use rust_iptables::iptables;
use std::process::{Command, Stdio};
//use tokio::process::Command;
use std::io;
use std::io::Write;

fn main() {
    println!("Hello, world!");

    let output = Command::new("iptables-save")
        .output()
        .expect("!iptables-save");
    println!("status: {}", output.status);
    println!("output formatted: {:?}", output);
    let saved_value = output.stdout;
    println!("read value: {:?}", saved_value);

    let mut child = Command::new("iptables-restore")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    println!("spawn'd it!");
    let mut stdin = child.stdin.take().expect("");
    println!("...spawn'd it!");
    std::thread::spawn(move || {
        if let Err(e) = stdin.write_all(&*saved_value) {
            println!("failure to write to stdin: {}", e.to_string())
        }
        println!("wrote iwrote iwrote i");
    });
    let output = child.wait_with_output().unwrap();

    println!("status: {}", output.status);


    // now let's get our shit together with respect to ownership, lifetime..
    let table_buf = handle_iptables_save().unwrap();//   let mut buf = handle_iptables_save().unwrap();

    handle_iptables_restore(table_buf);
}

fn handle_iptables_save() -> io::Result< Vec<u8> > {
    let output = Command::new("iptables-save").output().expect("!iptables-save");
    println!("status: {}", output.status);
    let buf = output.stdout;
    Ok(buf)
}

fn handle_iptables_restore(buf: Vec<u8>) {
    let mut child = Command::new("iptables-restore").stdin(Stdio::piped()).spawn().expect("!iptables-restore");
    let mut stdin = child.stdin.take().expect("");
    std::thread::spawn(move || {
        stdin.write_all(&*buf);
    });
    let output = child
        .wait_with_output()
        .expect("failed to wait on child");

    println!("we made it!")
}

