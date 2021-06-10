// Copyright (c) 2021 Apple Inc.
//
// SPDX-License-Identifier: Apache-2.0
//

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::fs;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::{self, Duration};

use anyhow::{ensure, Context, Result};
use async_recursion::async_recursion;
use nix::mount::{umount, MsFlags};
use slog::{debug, error, Logger};

use crate::mount::BareMount;
use crate::protocols::agent::Storage;

/// The maximum number of file system entries agent will watch for each mount.
const MAX_ENTRIES_PER_STORAGE: usize = 8;

/// How often to check for modified files.
const WATCH_INTERVAL_SECS: u64 = 2;

/// Destination path for tmpfs
const WATCH_MOUNT_POINT_PATH: &str = "/run/kata-containers/shared/containers/watchable/";

/// Represents a single storage entry (may have multiple files to watch).
#[derive(Default, Debug, Clone)]
struct Entry {
    source: PathBuf,
    mount_point: PathBuf,
    files: HashMap<PathBuf, SystemTime>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.mount_point);
    }
}

impl Entry {
    async fn new(storage: Storage) -> Result<Entry> {
        let source = PathBuf::from(&storage.source);
        let mut mount_point = PathBuf::from(&storage.mount_point);

        if source.is_file() && mount_point.is_dir() {
            let filename = source.file_name().with_context(|| {
                format!("Failed to extract file name from {}", source.display())
            })?;
            mount_point = mount_point.join(filename);
        }

        let entry = Entry {
            source,
            mount_point,
            files: HashMap::new(),
        };

        Ok(entry)
    }

    async fn update_target(
        &self,
        logger: &Logger,
        source_file_path: impl AsRef<Path>,
    ) -> Result<()> {
        let source_file_path = source_file_path.as_ref();
        let dest_file_path = self.make_dest_path(&source_file_path)?;

        if let Some(path) = dest_file_path.parent() {
            debug!(logger, "Creating destination directory: {}", path.display());
            fs::create_dir_all(path).await?;
        }

        debug!(
            logger,
            "Copy from {} to {}",
            source_file_path.display(),
            dest_file_path.display()
        );
        fs::copy(source_file_path, dest_file_path).await?;

        Ok(())
    }

    async fn scan(&mut self, logger: &Logger) -> Result<usize> {
        debug!(logger, "Scanning for changes");

        let mut remove_list = Vec::new();

        // Remove deleted files for tracking list
        self.files.retain(|st, _| {
            if st.exists() {
                true
            } else {
                remove_list.push(st.to_path_buf());
                false
            }
        });

        // Delete from target
        for path in remove_list {
            // Entry has been deleted, remove it from target mount
            let target = self.make_dest_path(path)?;
            debug!(logger, "Removing file from mount: {}", target.display());
            let _ = fs::remove_file(target).await;
        }

        // Scan new & changed files
        let count = self
            .scan_path(logger, self.source.clone().as_path())
            .await?;

        Ok(count)
    }

    #[async_recursion]
    async fn scan_path(&mut self, logger: &Logger, path: &Path) -> Result<usize> {
        let mut count = 0;

        debug!(logger, "Scanning path: {}", path.display());

        if path.is_file() {
            let modified = path.metadata()?.modified()?;

            ensure!(
                self.files.len() <= MAX_ENTRIES_PER_STORAGE,
                "Too many file system entries to watch (must be < {})",
                MAX_ENTRIES_PER_STORAGE
            );

            // Insert will return old entry if any
            if let Some(old_st) = self.files.insert(path.to_path_buf(), modified) {
                if modified > old_st {
                    // Modified date changed, update target
                    debug!(logger, "Updating file: {}", path.display());
                    self.update_target(logger, path).await?;
                    count += 1;
                }
            } else {
                // Entry just added, copy to target
                debug!(logger, "Copying new entry: {}", path.display());
                self.update_target(logger, path).await?;
                count += 1;
            }
        } else {
            // Scan dir recursively
            let mut entries = fs::read_dir(path)
                .await
                .with_context(|| format!("Failed to read dir: {}", path.display()))?;

            while let Some(entry) = entries.next_entry().await? {
                count += self.scan_path(logger, entry.path().as_path()).await?;
            }
        }

        Ok(count)
    }

    fn make_dest_path(&self, source_file_path: impl AsRef<Path>) -> Result<PathBuf> {
        let relative_path = source_file_path
            .as_ref()
            .strip_prefix(&self.source)
            .with_context(|| {
                format!(
                    "Failed to get prefix: {} - {}",
                    source_file_path.as_ref().display().to_string(),
                    &self.source.display()
                )
            })?;

        let dest_file_path = Path::new(&self.mount_point).join(relative_path);
        Ok(dest_file_path)
    }
}

#[derive(Default, Debug)]
struct Entries(Vec<Entry>);

impl Entries {
    async fn add(
        &mut self,
        list: impl IntoIterator<Item = Storage>,
        logger: &Logger,
    ) -> Result<()> {
        debug!(&logger, "entries add");
        for storage in list.into_iter() {
            let entry = Entry::new(storage).await?;
            self.0.push(entry);
        }
        debug!(logger, "adding an entry...");

        // Perform initial copy
        self.check(logger).await?;

        Ok(())
    }

    async fn check(&mut self, logger: &Logger) -> Result<()> {
        debug!(logger, "calling check");
        for entry in self.0.iter_mut() {
            entry.scan(logger).await?;
        }
        Ok(())
    }
}

/// Handles watchable mounts, the watcher keeps a list of files to monitor and periodically checks
/// the modified date of each file. When so, the watcher will copy changed files to a tmpfs mount.
/// This is a temporary workaround to handle config map updates until we get inotify on 9p/virtio-fs.
/// More context on this:
/// - https://github.com/kata-containers/runtime/issues/1505
/// - https://github.com/kata-containers/kata-containers/issues/1879
#[derive(Debug, Default)]
pub struct BindWatcher {
    /// Container ID -> Vec of watched entries
    shared: Arc<Mutex<HashMap<String, Entries>>>,
    watch_thread: Option<task::JoinHandle<()>>,
}

impl Drop for BindWatcher {
    fn drop(&mut self) {
        self.cleanup();
    }
}

impl BindWatcher {
    pub fn new() -> BindWatcher {
        Default::default()
    }

    pub async fn add_container(
        &mut self,
        id: String,
        mounts: impl IntoIterator<Item = Storage>,
        logger: &Logger,
    ) -> Result<()> {
        debug!(&logger, "add_container");
        if self.watch_thread.is_none() {
            debug!(&logger, "setting up the watcher");
            // Virtiofs shared path is RO by default, back it by tmpfs.
            self.mount(logger).await?;

            // Spawn background thread to monitor changes
            let join_handle = Self::spawn_watcher(
                logger.clone(),
                Arc::clone(&self.shared),
                WATCH_INTERVAL_SECS,
            );

            debug!(&logger, "the watcher has been spawned");
            self.watch_thread = Some(join_handle);
        }
        debug!(&logger, "ok, ready to add entry et al");

        self.shared
            .lock()
            .await
            .entry(id.to_owned())
            .or_insert_with(Entries::default)
            .add(mounts, logger)
            .await?;

        Ok(())
    }

    pub async fn remove_container(&self, id: &str) {
        self.shared.lock().await.remove(id);
    }

    fn spawn_watcher(
        logger: Logger,
        shared: Arc<Mutex<HashMap<String, Entries>>>,
        interval_secs: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                debug!(&logger, "Looking for changed files");
                for (_, entries) in shared.lock().await.iter_mut() {
                    if let Err(err) = entries.check(&logger).await {
                        // We don't fail background loop, but rather log error instead.
                        error!(logger, "Check failed: {}", err);
                    }
                }
            }
        })
    }

    async fn mount(&self, logger: &Logger) -> Result<()> {
        fs::create_dir_all(WATCH_MOUNT_POINT_PATH).await?;

        BareMount::new(
            "tmpfs",
            WATCH_MOUNT_POINT_PATH,
            "tmpfs",
            MsFlags::empty(),
            "",
            logger,
        )
        .mount()?;

        Ok(())
    }

    fn cleanup(&mut self) {
        if let Some(handle) = self.watch_thread.take() {
            // Stop our background thread
            handle.abort();
        }

        let _ = umount(WATCH_MOUNT_POINT_PATH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mount::is_mounted;
    use crate::skip_if_not_root;
    use std::fs;
    use std::thread;

    #[tokio::test]
    async fn watch_directory() {
        // Prepare source directory:
        // ./tmp/1.txt
        // ./tmp/A/B/2.txt
        let source_dir = tempfile::tempdir().unwrap();
        fs::write(source_dir.path().join("1.txt"), "one").unwrap();
        fs::create_dir_all(source_dir.path().join("A/B")).unwrap();
        fs::write(source_dir.path().join("A/B/1.txt"), "two").unwrap();

        let dest_dir = tempfile::tempdir().unwrap();

        let mut entry = Entry::new(Storage {
            source: source_dir.path().display().to_string(),
            mount_point: dest_dir.path().display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        let logger = slog::Logger::root(slog::Discard, o!());

        assert_eq!(entry.scan(&logger).await.unwrap(), 2);

        // Should copy no files since nothing is changed since last check
        assert_eq!(entry.scan(&logger).await.unwrap(), 0);

        // Should copy 1 file
        thread::sleep(Duration::from_secs(1));
        fs::write(source_dir.path().join("A/B/1.txt"), "updated").unwrap();
        assert_eq!(entry.scan(&logger).await.unwrap(), 1);
        assert_eq!(
            fs::read_to_string(dest_dir.path().join("A/B/1.txt")).unwrap(),
            "updated"
        );

        // Should copy no new files after copy happened
        assert_eq!(entry.scan(&logger).await.unwrap(), 0);

        // Update another file
        fs::write(source_dir.path().join("1.txt"), "updated").unwrap();
        assert_eq!(entry.scan(&logger).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn watch_file() {
        let source_dir = tempfile::tempdir().unwrap();
        fs::write(source_dir.path().join("1.txt"), "one").unwrap();

        let dest_dir = tempfile::tempdir().unwrap();

        let mut entry = Entry::new(Storage {
            source: source_dir.path().display().to_string(),
            mount_point: dest_dir.path().display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        let logger = slog::Logger::root(slog::Discard, o!());

        assert_eq!(entry.scan(&logger).await.unwrap(), 1);

        thread::sleep(Duration::from_secs(1));
        fs::write(source_dir.path().join("1.txt"), "two").unwrap();
        assert_eq!(entry.scan(&logger).await.unwrap(), 1);
        assert_eq!(
            fs::read_to_string(dest_dir.path().join("1.txt")).unwrap(),
            "two"
        );
        assert_eq!(entry.scan(&logger).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn delete_file() {
        let source_dir = tempfile::tempdir().unwrap();
        let source_file = source_dir.path().join("1.txt");
        fs::write(&source_file, "one").unwrap();

        let dest_dir = tempfile::tempdir().unwrap();
        let target_file = dest_dir.path().join("1.txt");

        let mut entry = Entry::new(Storage {
            source: source_dir.path().display().to_string(),
            mount_point: dest_dir.path().display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        let logger = slog::Logger::root(slog::Discard, o!());

        assert_eq!(entry.scan(&logger).await.unwrap(), 1);
        assert_eq!(entry.files.len(), 1);

        assert!(target_file.exists());
        assert!(entry.files.contains_key(&source_file));

        // Remove source file
        fs::remove_file(&source_file).unwrap();

        assert_eq!(entry.scan(&logger).await.unwrap(), 0);

        assert_eq!(entry.files.len(), 0);
        assert!(!target_file.exists());
    }

    #[tokio::test]
    async fn make_dest_path() {
        let source_dir = tempfile::tempdir().unwrap();
        let target_dir = tempfile::tempdir().unwrap();

        let source_dir = source_dir.path();
        let target_dir = target_dir.path();

        let entry = Entry::new(Storage {
            source: source_dir.display().to_string(),
            mount_point: target_dir.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        assert_eq!(
            entry.make_dest_path(source_dir.join("1.txt")).unwrap(),
            target_dir.join("1.txt")
        );

        assert_eq!(
            entry.make_dest_path(source_dir.join("a/b/2.txt")).unwrap(),
            target_dir.join("a/b/2.txt")
        );
    }

    #[tokio::test]
    async fn create_tmpfs() {
        skip_if_not_root!();

        let logger = slog::Logger::root(slog::Discard, o!());
        let mut watcher = BindWatcher::default();

        watcher.mount(&logger).await.unwrap();
        assert!(is_mounted(WATCH_MOUNT_POINT_PATH).unwrap());

        watcher.cleanup();
        assert!(!is_mounted(WATCH_MOUNT_POINT_PATH).unwrap());
    }

    #[tokio::test]
    async fn spawn_thread() {
        skip_if_not_root!();

        let source_dir = tempfile::tempdir().unwrap();
        fs::write(source_dir.path().join("1.txt"), "one").unwrap();

        let dest_dir = tempfile::tempdir().unwrap();

        let storage = Storage {
            source: source_dir.path().display().to_string(),
            mount_point: dest_dir.path().display().to_string(),
            ..Default::default()
        };

        let logger = slog::Logger::root(slog::Discard, o!());
        let mut watcher = BindWatcher::default();

        watcher
            .add_container("test".into(), std::iter::once(storage), &logger)
            .await
            .unwrap();

        thread::sleep(Duration::from_secs(WATCH_INTERVAL_SECS));

        let out = fs::read_to_string(dest_dir.path().join("1.txt")).unwrap();
        assert_eq!(out, "one");
    }
}
