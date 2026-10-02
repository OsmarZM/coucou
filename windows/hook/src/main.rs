//! coucou-hook — a small relay for Claude, Codex, Gemini and Copilot CLI hooks.
//!
//! Reads bounded JSON stdin under a deadline, builds an allowlisted event and
//! sends it to `\\.\pipe\coucou-<sid>` after checking the server's user. Only
//! inspectable Claude PermissionRequest events wait for an explicit UI answer.
//! Silence/timeout/failure always returns to the CLI's native permission flow.
//!
//! Legacy: `coucou-hook <EventName>` (Claude).
//! Multiagent: `coucou-hook --agent codex --event PreToolUse`.

use coucou_agent_protocol::{normalize, Agent, AgentEvent, MAX_PAYLOAD_BYTES};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

const INPUT_BUDGET: Duration = Duration::from_millis(500);
const MAX_STDIN_BYTES: usize = 1024 * 1024;
const MAX_DECISION_BYTES: usize = 128;
const FIRE_AND_FORGET_BUDGET: Duration = Duration::from_millis(1200);
const DECISION_BUDGET: Duration = Duration::from_secs(110);

#[cfg(windows)]
mod win;

#[derive(Clone, Debug, PartialEq)]
struct Invocation {
    agent: Agent,
    event: String,
}

fn parse_args(args: &[String]) -> Option<Invocation> {
    let mut agent = None;
    let mut event = None;
    let mut legacy_event = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--agent" if agent.is_none() => {
                index += 1;
                agent = Some(Agent::from_name(args.get(index)?)?);
            }
            "--event" if event.is_none() => {
                index += 1;
                let value = args.get(index)?;
                if value.is_empty() || value.starts_with('-') {
                    return None;
                }
                event = Some(value.clone());
            }
            "--coucou-managed-v1" => {}
            value if !value.starts_with('-') && legacy_event.is_none() => {
                legacy_event = Some(value.to_owned());
            }
            _ => return None,
        }
        index += 1;
    }
    if legacy_event.is_some() && (agent.is_some() || event.is_some()) {
        return None;
    }
    Some(Invocation {
        agent: agent.unwrap_or(Agent::Claude),
        // Old integrations may provide the name exclusively in native stdin.
        event: event.or(legacy_event).unwrap_or_default(),
    })
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    // Even malformed Gemini invocations must emit valid neutral JSON instead
    // of turning a monitoring error into a warning/blocking response.
    let response_agent = args
        .windows(2)
        .find(|pair| pair[0] == "--agent")
        .and_then(|pair| Agent::from_name(&pair[1]))
        .unwrap_or(Agent::Claude);
    let decision = parse_args(&args).and_then(run);
    if let Some(json) = response_json(response_agent, decision.as_deref()) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{json}");
        let _ = out.flush();
    }
    // Detached workers can be stuck in stdin/pipe I/O. Process termination
    // closes their handles instead of waiting for them after the deadline.
    std::process::exit(0);
}

fn run(invocation: Invocation) -> Option<String> {
    let event = read_event_with_deadline(invocation)?;
    let waits_for_answer = event.requires_approval;
    let mut line = serde_json::to_string(&event).ok()?;
    if line.len() >= MAX_PAYLOAD_BYTES {
        return None;
    }
    line.push('\n');
    let budget = if waits_for_answer {
        DECISION_BUDGET
    } else {
        FIRE_AND_FORGET_BUDGET
    };
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("coucou-relay".into())
        .spawn(move || {
            let _ = tx.send(talk(&line, waits_for_answer));
        })
        .ok()?;
    rx.recv_timeout(budget).ok().flatten()
}

fn read_event_with_deadline(invocation: Invocation) -> Option<AgentEvent> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("coucou-hook-input".into())
        .spawn(move || {
            let event = read_event(std::io::stdin().lock(), &invocation);
            let _ = tx.send(event);
        })
        .ok()?;
    rx.recv_timeout(INPUT_BUDGET).ok().flatten()
}

fn read_event(reader: impl Read, invocation: &Invocation) -> Option<AgentEvent> {
    let mut raw = Vec::new();
    reader
        .take((MAX_STDIN_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .ok()?;
    if raw.is_empty() || raw.len() > MAX_STDIN_BYTES {
        return None;
    }
    // Some shells prefix JSON with a UTF-8 BOM.
    let bytes = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&raw);
    let mut payload = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    if !payload.is_object() {
        return None;
    }
    // Preserve the existing project fallback, never use cwd as session identity.
    if payload
        .get("cwd")
        .and_then(|value| value.as_str())
        .is_none_or(str::is_empty)
    {
        if let Ok(cwd) = std::env::current_dir() {
            payload["cwd"] = serde_json::Value::String(cwd.to_string_lossy().into_owned());
        }
    }
    normalize(invocation.agent, &invocation.event, &payload)
        .ok()
        .flatten()
}

/// Gemini requires JSON stdout. Empty objects grant no permission and modify
/// no flow-control fields. Codex/Copilot/Claude observation hooks remain silent.
fn response_json(agent: Agent, decision: Option<&str>) -> Option<String> {
    if agent == Agent::Gemini {
        return Some("{}".into());
    }
    if agent != Agent::Claude {
        return None;
    }
    let behavior = match decision?.trim() {
        // Remembering an allowance is the island's concern, never the relay's.
        "allow" | "always" => r#"{"behavior":"allow"}"#,
        "deny" => r#"{"behavior":"deny","message":"Denied from Coucou"}"#,
        _ => return None,
    };
    Some(format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{behavior}}}}}"#
    ))
}

#[cfg(windows)]
fn connect() -> Option<std::fs::File> {
    use std::os::windows::io::AsRawHandle;
    use std::time::Instant;
    const ERROR_PIPE_BUSY: i32 = 231;
    const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
    // Fail closed if our SID is unavailable: a user-name pipe fallback could
    // collide with another account and is unnecessary for supported Windows.
    let sid = win::current_user_sid()?;
    let path = format!(r"\\.\pipe\coucou-{sid}");
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => {
                let handle = windows::Win32::Foundation::HANDLE(file.as_raw_handle());
                return win::pipe_server_is_same_user(handle).then_some(file);
            }
            Err(err) => {
                if err.raw_os_error() != Some(ERROR_PIPE_BUSY) || Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
}

#[cfg(not(windows))]
fn connect() -> Option<std::fs::File> {
    None
}

fn talk(payload: &str, waits_for_answer: bool) -> Option<String> {
    let mut pipe = connect()?;
    exchange(&mut pipe, payload, waits_for_answer)
}

fn exchange(
    pipe: &mut (impl Read + Write),
    payload: &str,
    waits_for_answer: bool,
) -> Option<String> {
    pipe.write_all(payload.as_bytes()).ok()?;
    pipe.flush().ok()?;
    if !waits_for_answer {
        return None;
    }
    // A broken/spoofed/overlong reply must never become an allow. The backend
    // emits a single bounded word followed by a newline.
    let mut buf = Vec::new();
    let mut chunk = [0u8; MAX_DECISION_BYTES + 1];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > MAX_DECISION_BYTES {
                    return None;
                }
                if buf.contains(&b'\n') {
                    break;
                }
            }
            Err(_) => return None,
        }
    }
    let answer = std::str::from_utf8(&buf).ok()?.trim();
    matches!(answer, "allow" | "deny" | "always").then(|| answer.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn legacy_and_multiagent_arguments_preserve_event_domains() {
        assert_eq!(
            parse_args(&args(&["PermissionRequest"])),
            Some(Invocation {
                agent: Agent::Claude,
                event: "PermissionRequest".into()
            })
        );
        assert_eq!(
            parse_args(&args(&[
                "--agent",
                "copilot",
                "--event",
                "preToolUse",
                "--coucou-managed-v1"
            ])),
            Some(Invocation {
                agent: Agent::Copilot,
                event: "preToolUse".into()
            })
        );
        for invalid in [
            args(&["--agent", "unknown"]),
            args(&["--agent"]),
            args(&["--event"]),
            args(&["Stop", "--agent", "codex"]),
            args(&["--agent", "gemini", "--agent", "claude"]),
            args(&["--unknown"]),
        ] {
            assert!(parse_args(&invalid).is_none());
        }
    }

    #[test]
    fn malformed_and_oversized_stdin_is_neutral() {
        let invocation = Invocation {
            agent: Agent::Claude,
            event: "SessionStart".into(),
        };
        for raw in [
            b"".as_slice(),
            b"not json",
            b"[]",
            b"{}",
            b"{\"session_id\":4}",
            &vec![b' '; MAX_STDIN_BYTES + 1],
        ] {
            assert!(read_event(raw, &invocation).is_none());
        }
        let original =
            json!({"session_id":"s", "hook_event_name":"SessionStart", "prompt":"SECRET"})
                .to_string();
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(original.as_bytes());
        let event = read_event(bom.as_slice(), &invocation).unwrap();
        assert!(!serde_json::to_string(&event).unwrap().contains("SECRET"));
    }

    #[test]
    fn provider_neutral_output_never_grants_other_agents_permission() {
        for decision in [
            None,
            Some("allow"),
            Some("deny"),
            Some("always"),
            Some("unknown"),
        ] {
            assert_eq!(
                response_json(Agent::Gemini, decision).as_deref(),
                Some("{}")
            );
            assert!(response_json(Agent::Codex, decision).is_none());
            assert!(response_json(Agent::Copilot, decision).is_none());
        }
        assert!(response_json(Agent::Claude, None).is_none());
        assert!(response_json(Agent::Claude, Some("maybe")).is_none());
        assert!(response_json(Agent::Claude, Some(r#"{"permissionDecision":"allow"}"#)).is_none());
    }

    #[test]
    fn claude_explicit_decision_matches_native_permission_output() {
        assert_eq!(
            response_json(Agent::Claude, Some("allow")).unwrap(),
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
        );
        assert_eq!(
            response_json(Agent::Claude, Some("deny")).unwrap(),
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied from Coucou"}}}"#
        );
        assert!(response_json(Agent::Claude, Some("always"))
            .unwrap()
            .contains(r#""behavior":"allow""#));
    }

    struct TestPipe {
        answer: std::io::Cursor<Vec<u8>>,
        written: Vec<u8>,
        fail_write: bool,
        reads: usize,
    }

    impl Read for TestPipe {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            self.answer.read(output)
        }
    }

    impl Write for TestPipe {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail_write {
                Err(std::io::ErrorKind::BrokenPipe.into())
            } else {
                self.written.extend_from_slice(bytes);
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn broken_oversized_empty_and_malformed_replies_never_become_allow() {
        for reply in [
            "".into(),
            "unknown\n".into(),
            "allow\nmalformed".into(),
            format!("allow{}\n", " ".repeat(MAX_DECISION_BYTES)),
            "{\"decision\":\"allow\"}\n".into(),
        ] {
            let mut pipe = TestPipe {
                answer: std::io::Cursor::new(reply.into_bytes()),
                written: Vec::new(),
                fail_write: false,
                reads: 0,
            };
            assert!(exchange(&mut pipe, "event\n", true).is_none());
        }
        let mut broken = TestPipe {
            answer: std::io::Cursor::new(b"allow\n".to_vec()),
            written: Vec::new(),
            fail_write: true,
            reads: 0,
        };
        assert!(exchange(&mut broken, "event\n", true).is_none());
        assert_eq!(broken.reads, 0);
    }

    #[test]
    fn observation_never_reads_or_uses_a_permission_response() {
        let mut pipe = TestPipe {
            answer: std::io::Cursor::new(b"allow\n".to_vec()),
            written: Vec::new(),
            fail_write: false,
            reads: 0,
        };
        assert!(exchange(&mut pipe, "normalized event\n", false).is_none());
        assert_eq!(pipe.written, b"normalized event\n");
        assert_eq!(pipe.reads, 0);
        assert_eq!(
            exchange(&mut pipe, "permission\n", true).as_deref(),
            Some("allow")
        );
    }
}
