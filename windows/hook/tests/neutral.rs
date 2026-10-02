//! Real executable tests exercise the main-thread deadline and stdout contract.
//! Inputs here are invalid/missing native IDs, so no event reaches a user's app.

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run(args: &[&str], input: Option<&[u8]>, leave_stdin_open: bool) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_coucou-hook"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        // Oversized input may make the child exit before the producer finishes;
        // a broken stdin pipe is the expected result in that rejection case.
        let _ = child.stdin.as_mut().unwrap().write_all(input);
    }
    if !leave_stdin_open {
        drop(child.stdin.take());
    }
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("monitoring hook did not release its CLI before deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn invalid_payloads_preserve_each_providers_neutral_stdout() {
    for agent in ["claude", "codex", "gemini", "copilot"] {
        for raw in [
            b"not json".as_slice(),
            b"[]",
            b"{}",
            b"{\"session_id\":\"\"}",
        ] {
            let output = run(
                &["--agent", agent, "--event", "SessionStart"],
                Some(raw),
                false,
            );
            assert!(output.status.success());
            assert!(output.stderr.is_empty());
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                if agent == "gemini" { "{}" } else { "" }
            );
        }
    }
}

#[test]
fn stdin_without_eof_cannot_hold_permission_or_teardown_hooks() {
    for args in [
        vec!["PermissionRequest"],
        vec!["--agent", "codex", "--event", "Interrupt"],
        vec!["--agent", "gemini", "--event", "BeforeTool"],
    ] {
        let started = Instant::now();
        let output = run(&args, None, true);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            if args.contains(&"gemini") { "{}" } else { "" }
        );
    }
}

#[test]
fn oversized_input_and_malformed_cli_arguments_exit_successfully() {
    let output = run(
        &["--agent", "gemini", "--event", "BeforeTool"],
        Some(&vec![b' '; 1024 * 1024 + 1]),
        false,
    );
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "{}");
    for args in [
        vec!["--unknown"],
        vec!["--agent", "gemini", "--event"],
        vec!["--agent", "codex", "--agent", "claude"],
    ] {
        let output = run(&args, Some(b"{}"), false);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            if args.contains(&"gemini") { "{}" } else { "" }
        );
    }
}
