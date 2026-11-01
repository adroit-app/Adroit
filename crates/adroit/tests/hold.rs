//! The gate a held program waits behind on Windows: the adroit executable
//! itself, which runs the program once a line comes.
#![cfg(windows)]

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn gate(program: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_adroit"))
        .arg("__hold")
        .args(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn released(mut gate: Child) -> std::process::Output {
    // A gate that refuses has ended before it is written to.
    let _ = gate.stdin.take().unwrap().write_all(b"\r\n");
    gate.wait_with_output().unwrap()
}

#[test]
fn a_held_program_waits_for_a_line_then_draws_from_the_top_of_the_screen() {
    let mut held = gate(&["cmd", "/c", "echo started"]);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        held.try_wait().unwrap().is_none(),
        "ran before it was released"
    );
    let out = released(held);
    assert!(out.status.success());
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(printed.starts_with("\x1b[H"), "{printed:?}");
    assert!(printed.contains("started"), "{printed:?}");
}

#[test]
fn the_gate_ends_with_the_exit_code_of_its_program() {
    let out = released(gate(&["cmd", "/c", "exit 3"]));
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn a_program_that_cannot_start_ends_the_gate_as_a_shell_would() {
    let out = released(gate(&["no-such-program-anywhere"]));
    assert_eq!(out.status.code(), Some(127));
}

#[test]
fn a_gate_holding_nothing_refuses() {
    let out = released(gate(&[]));
    assert_eq!(out.status.code(), Some(2));
}
