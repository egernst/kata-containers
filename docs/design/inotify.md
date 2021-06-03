# Kata Containers support for `inotify`

## Background on `inotify` usage

A common pattern in Kubernetes is to watch for changes to files/directories passed in as `ConfigMaps`
or `Secrets`. Sidecar's normally use `inotify` to watch for changes and then signal the primary container to reload
the updated configuration. Kata Containers typically will pass these host files into the guest using `virtiofs`, which
does not support `inotify`. This document describes how Kata Containers works around this limitation.

### Detecting a `watchable` mount

Kubernetes creates `secrets` and `ConfigMap` mounts at very specific locations on the host filesystem. For container mounts,
the `Kata Containers` runtime will check the source of the mount to identify these special cases. For these use cases, only a single file
or very few would typically need to be watched. To avoid excessive overheads in making a mount watchable,
we enforce a limit of 8 files per mount. If a `secret` or `ConfigMap` mount contains more than 8 files, it will not be
considered watchable. In the non-watchable scenario, updates to the mount continue to be propagated to the container workload,
but these updates will not trigger an `inotify` event.

### Presenting a `watchable` mount to the workload

For mounts that are considered `watchable`, inside the guest, the `kata-agent` will poll the mount presented from
the host through `virtiofs` and copy any changed files to a `tmpfs` mount that is presented to the container. In this way,
`Kata` will do the polling on behalf of the workload and existing workloads needn't change their usage of `inotify`.

![drawing](arch-images/inotify-workaround.png)
