//! Claude Code stream-json transport, with the stdio control envelope used by
//! Anthropic's official Agent SDK. This first adapter exposes consultative
//! tools only; writable Claude sessions need a separately qualified SDK path.

use std::time::{Duration, Instant};

use crate::usage::{activity, Collector};
use serde_json::{json, Value};

use super::{transport::ProcessIo, RunContext, RunOutcome};

const MAX_TEXT_BYTES: usize = 1_000_000;

pub fn args(ctx: &RunContext) -> Result<Vec<String>, String> {
    if ctx.writable {
        return Err("Claude CLI chat currently supports consultative mode only. Use Codex for approved project changes.".into());
    }
    let mut args = [
        "--print",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        "default",
        "--permission-prompt-tool",
        "stdio",
        "--tools",
        "Read,Glob,Grep",
        "--disallowedTools",
        "mcp__*",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--setting-sources",
        "",
        "--settings",
        "{\"disableAllHooks\":true}",
        "--disable-slash-commands",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if ctx.personal {
        let index = args
            .iter()
            .position(|value| value == "--tools")
            .ok_or("Opção de ferramentas indisponível.")?;
        args[index + 1] = String::new();
    }
    if let Some(id) = &ctx.session_id {
        if !valid_uuid(id) {
            return Err(
                "Claude CLI resume requires a valid session UUID. Start a new conversation.".into(),
            );
        }
        args.push(format!("--resume={id}"));
    }
    Ok(args)
}

pub async fn run(io: &mut ProcessIo, ctx: &RunContext) -> Result<RunOutcome, String> {
    // Keep the same validation even when a caller constructs the process itself.
    args(ctx)?;
    initialize(io).await?;
    io.send(json!({
        "type": "user", "session_id": ctx.session_id.as_deref().unwrap_or(""),
        "message": { "role": "user", "content": [{ "type": "text", "text": ctx.query }] },
        "parent_tool_use_id": null
    }))
    .await?;
    let mut session_id = ctx.session_id.clone().unwrap_or_default();
    let mut text = String::new();
    let mut assistant_text = String::new();
    let mut usage: Option<Collector> = None;
    let mut subagents = std::collections::HashSet::new();
    loop {
        let message = match io.next().await {
            Ok(message) => message,
            Err(_) if io.is_cancelled() => {
                let _ = io
                    .send(
                        json!({ "type": "control_request", "request_id": "coucou-interrupt",
                    "request": { "subtype": "interrupt" } }),
                    )
                    .await;
                let _ = tokio::time::timeout(Duration::from_secs(2), cancellation_grace(io)).await;
                return Ok(RunOutcome {
                    session_id,
                    status: "interrupted".into(),
                    text,
                });
            }
            Err(err) => return Err(err),
        };
        if message.get("type").and_then(Value::as_str) == Some("control_request") {
            control(io, &message).await?;
            continue;
        }
        if let Some(id) = message.get("session_id").and_then(Value::as_str) {
            if valid_uuid(id) {
                if session_id.is_empty() {
                    session_id = id.to_string();
                    io.emit("session", json!({ "sessionId": session_id }));
                } else if session_id != id {
                    return Err("Claude CLI returned an event for another session.".into());
                }
            } else if !id.is_empty() {
                return Err("Claude CLI returned an invalid session ID.".into());
            }
        }
        if !session_id.is_empty() {
            let collector =
                usage.get_or_insert_with(|| Collector::new("claude", &session_id, &ctx.run_id));
            if let Some(version) = message.get("claude_code_version").and_then(Value::as_str) {
                collector.version(Some(version));
            }
            collector.claude_message(&message);
            if let Some(snapshot) = collector.take_update() {
                io.emit("usage", snapshot);
            }
            for event in activities(&message, &session_id, &ctx.run_id, &mut subagents) {
                io.emit("activity", event);
            }
        }
        // Do not concatenate nested-agent text into the selected conversation.
        if message
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            continue;
        }
        match message.get("type").and_then(Value::as_str) {
            Some("stream_event") => {
                if let Some(delta) = text_delta(&message) {
                    append(&mut text, delta)?;
                    io.emit("delta", json!({ "text": delta }));
                } else if let Some(name) = stream_tool(&message) {
                    io.emit(
                        "tool",
                        json!({ "text": format!("Claude: {name}"), "status": "in_progress" }),
                    );
                }
            }
            Some("assistant") => {
                // The CLI emits full assistant frames as well as token deltas.
                // Accumulate them separately to avoid duplicating streamed text.
                if let Some(content) = message
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                {
                    for block in content {
                        if block.get("type").and_then(Value::as_str) == Some("text") {
                            if let Some(value) = block.get("text").and_then(Value::as_str) {
                                append(&mut assistant_text, value)?;
                            }
                        } else if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("read tool");
                            io.emit("tool", json!({ "text": format!("Claude: {}", name.chars().take(100).collect::<String>()), "status": "in_progress" }));
                        }
                    }
                }
            }
            Some("result") => {
                if message.get("is_error").and_then(Value::as_bool) == Some(true)
                    || message.get("subtype").and_then(Value::as_str) != Some("success")
                {
                    return Err("Claude CLI could not complete the turn. Verify its login and version in the terminal.".into());
                }
                if !valid_uuid(&session_id) {
                    return Err("Claude CLI completed without a valid session ID.".into());
                }
                if let Some(final_text) = message
                    .get("result")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    if final_text.len() > MAX_TEXT_BYTES {
                        return Err("The CLI response exceeded the chat size limit.".into());
                    }
                    text = final_text.to_string();
                } else if text.is_empty() {
                    text = assistant_text;
                }
                return Ok(RunOutcome {
                    session_id,
                    status: "completed".into(),
                    text,
                });
            }
            // System initialization and usage metadata never become chat text.
            _ => {}
        }
    }
}

fn activities(
    message: &Value,
    session: &str,
    run: &str,
    subagents: &mut std::collections::HashSet<String>,
) -> Vec<Value> {
    if message.get("session_id").and_then(Value::as_str) != Some(session) {
        return Vec::new();
    }
    let mut events = Vec::new();
    let mut record = |kind: &str, id: &str, status: &str, source: &str| {
        if let Some(event) = activity("claude", session, run, kind, id, status, source) {
            events.push(event);
        }
    };
    match message.get("type").and_then(Value::as_str) {
        Some("stream_event")
            if message.pointer("/event/type").and_then(Value::as_str)
                == Some("content_block_start")
                && message
                    .pointer("/event/content_block/type")
                    .and_then(Value::as_str)
                    == Some("tool_use") =>
        {
            if let Some(id) = message
                .pointer("/event/content_block/id")
                .and_then(Value::as_str)
            {
                record("tool", id, "running", "stream-json/stream_event.tool_use");
            }
        }
        Some("assistant") | Some("user") => {
            if let Some(blocks) = message
                .pointer("/message/content")
                .and_then(Value::as_array)
            {
                for block in blocks.iter().take(128) {
                    match block.get("type").and_then(Value::as_str) {
                        Some("tool_use") => {
                            if let Some(id) = block.get("id").and_then(Value::as_str) {
                                record("tool", id, "running", "stream-json/assistant.tool_use");
                            }
                        }
                        Some("tool_result") => {
                            if let Some(id) = block.get("tool_use_id").and_then(Value::as_str) {
                                record(
                                    "tool",
                                    id,
                                    if block.get("is_error").and_then(Value::as_bool) == Some(true)
                                    {
                                        "failed"
                                    } else {
                                        "completed"
                                    },
                                    "stream-json/user.tool_result",
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Some("system") => {
            let Some(id) = message.get("task_id").and_then(Value::as_str).filter(|id| {
                !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
            }) else {
                return events;
            };
            match message.get("subtype").and_then(Value::as_str) {
                // A generic background task can be a Bash/process job; it is
                // only a subagent when the provider explicitly identifies it.
                Some("task_started")
                    if message.get("task_type").and_then(Value::as_str) == Some("local_agent")
                        && subagents.len() < 128 =>
                {
                    subagents.insert(id.into());
                    record("subagent", id, "running", "stream-json/system.task_started");
                }
                Some("task_progress") if subagents.contains(id) => record(
                    "subagent",
                    id,
                    "running",
                    "stream-json/system.task_progress",
                ),
                Some("task_notification") if subagents.contains(id) => {
                    record(
                        "subagent",
                        id,
                        message
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        "stream-json/system.task_notification",
                    );
                    subagents.remove(id);
                }
                Some("task_updated") if subagents.contains(id) => {
                    let status = message
                        .pointer("/patch/status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    record("subagent", id, status, "stream-json/system.task_updated");
                    if matches!(status, "completed" | "failed" | "killed" | "stopped") {
                        subagents.remove(id);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
    events
}

async fn cancellation_grace(io: &mut ProcessIo) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let Ok(message) = io
            .next_uncancelled(deadline.saturating_duration_since(Instant::now()))
            .await
        else {
            break;
        };
        if message.get("type").and_then(Value::as_str) == Some("result")
            || (message.get("type").and_then(Value::as_str) == Some("control_response")
                && message
                    .pointer("/response/request_id")
                    .and_then(Value::as_str)
                    == Some("coucou-interrupt"))
        {
            break;
        }
        if message.get("type").and_then(Value::as_str) == Some("control_request") {
            let _ = deny_control(io, &message).await;
        }
    }
}

async fn initialize(io: &mut ProcessIo) -> Result<(), String> {
    io.send(
        json!({ "type": "control_request", "request_id": "coucou-initialize",
            "request": { "subtype": "initialize", "hooks": null }
        }),
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let message = io.next().await?;
            if message.get("type").and_then(Value::as_str) == Some("control_response")
                && message.pointer("/response/request_id").and_then(Value::as_str) == Some("coucou-initialize") {
                if message.pointer("/response/subtype").and_then(Value::as_str) != Some("success") {
                    return Err("The installed Claude CLI does not support its stdio control protocol. Update the CLI.".into());
                }
                return Ok(());
            }
            if message.get("type").and_then(Value::as_str) == Some("control_request") {
                // No user turn is active before initialization; reject callbacks.
                deny_control(io, &message).await?;
            }
        }
    }).await.map_err(|_| "Claude CLI control initialization timed out. Verify its version and login.".to_string())?
}

async fn control(io: &mut ProcessIo, message: &Value) -> Result<(), String> {
    let Some(request_id) = message
        .get("request_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
    else {
        return Err("Claude CLI returned an invalid control request ID.".into());
    };
    if message.pointer("/request/subtype").and_then(Value::as_str) != Some("can_use_tool") {
        return deny_control(io, message).await;
    }
    let mut decision = "deny".to_string();
    if let Some(details) = permission_details(&message["request"]) {
        if !io.is_cancelled() {
            decision = io.approve(details).await.unwrap_or_else(|_| "deny".into());
        }
    }
    let response = if decision == "allow" && !io.is_cancelled() {
        // No updatedPermissions: every response applies to this invocation only.
        json!({ "behavior": "allow", "updatedInput": message["request"]["input"] })
    } else {
        json!({ "behavior": "deny", "message": "Tool request declined in Coucou." })
    };
    io.send(control_success(request_id, response)).await
}

async fn deny_control(io: &mut ProcessIo, message: &Value) -> Result<(), String> {
    let Some(request_id) = message
        .get("request_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
    else {
        return Err("Claude CLI returned an invalid control request ID.".into());
    };
    if message.pointer("/request/subtype").and_then(Value::as_str) == Some("can_use_tool") {
        io.send(control_success(
            request_id,
            json!({ "behavior": "deny", "message": "No active approved chat turn." }),
        ))
        .await
    } else {
        io.send(json!({ "type": "control_response", "response": {
            "subtype": "error", "request_id": request_id, "error": "Unsupported control callback"
        } }))
        .await
    }
}

fn control_success(id: &str, response: Value) -> Value {
    json!({ "type": "control_response", "response": {
        "subtype": "success", "request_id": id, "response": response
    } })
}

fn permission_details(request: &Value) -> Option<Value> {
    let tool = request.get("tool_name")?.as_str()?;
    if !matches!(tool, "Read" | "Glob" | "Grep") {
        return None;
    }
    let input = request.get("input")?.as_object()?;
    const FIELDS: &[&str] = &[
        "file_path",
        "path",
        "pattern",
        "glob",
        "type",
        "output_mode",
        "offset",
        "limit",
        "pages",
        "head_limit",
        "multiline",
        "context",
        "-A",
        "-B",
        "-C",
        "-n",
        "-i",
    ];
    if input.is_empty() || input.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return None;
    }
    if input.values().any(|value| {
        !(value.is_string() || value.is_number() || value.is_boolean() || value.is_null())
    }) {
        return None;
    }
    let detail = serde_json::to_string_pretty(input).ok()?;
    if detail.len() > 16_000 {
        return None;
    }
    let lower = detail.to_ascii_lowercase();
    if [
        "authorization",
        "bearer ",
        "api_key",
        "access_token",
        "refresh_token",
        "client_secret",
        "private key",
        "sk-ant-",
        "github_pat_",
        "ghp_",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return None;
    }
    Some(json!({ "title": format!("Claude: {tool}"), "detail": detail }))
}

fn text_delta(message: &Value) -> Option<&str> {
    if message.pointer("/event/type").and_then(Value::as_str) != Some("content_block_delta")
        || message.pointer("/event/delta/type").and_then(Value::as_str) != Some("text_delta")
    {
        return None;
    }
    message.pointer("/event/delta/text")?.as_str()
}

fn stream_tool(message: &Value) -> Option<String> {
    if message.pointer("/event/type").and_then(Value::as_str) != Some("content_block_start")
        || message
            .pointer("/event/content_block/type")
            .and_then(Value::as_str)
            != Some("tool_use")
    {
        return None;
    }
    Some(
        message
            .pointer("/event/content_block/name")?
            .as_str()?
            .chars()
            .take(100)
            .collect(),
    )
}

fn valid_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn append(text: &mut String, delta: &str) -> Result<(), String> {
    if text.len().saturating_add(delta.len()) > MAX_TEXT_BYTES {
        return Err("The CLI response exceeded the chat size limit.".into());
    }
    text.push_str(delta);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> RunContext {
        RunContext {
            conversation_id: "c".into(),
            run_id: "r".into(),
            agent: "claude".into(),
            cwd: std::path::PathBuf::from("D:\\project"),
            session_id: None,
            query: "hello".into(),
            writable: false,
            personal: false,
            context_id: None,
        }
    }

    #[test]
    fn launch_restricts_tools_and_does_not_grant_write_or_shell_permissions() {
        let mut ctx = context();
        let argv = args(&ctx).unwrap();
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--tools", "Read,Glob,Grep"]));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "default"]));
        assert!(!argv.iter().any(|arg| arg == "--allowedTools"
            || arg.starts_with("--allowedTools=")
            || arg.contains("skip-permissions")
            || arg == "--bare"));
        ctx.writable = true;
        assert!(args(&ctx).is_err());
    }

    #[test]
    fn resume_is_a_uuid_and_never_an_option_or_transcript_path() {
        let mut ctx = context();
        for invalid in [
            "--dangerously-skip-permissions",
            "D:\\x.jsonl",
            "latest",
            " ",
        ] {
            ctx.session_id = Some(invalid.into());
            assert!(args(&ctx).is_err());
        }
        ctx.session_id = Some("550e8400-e29b-41d4-a716-446655440000".into());
        assert!(args(&ctx)
            .unwrap()
            .contains(&"--resume=550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[test]
    fn approval_exposes_full_known_read_input_and_rejects_shell_or_opaque_inputs() {
        assert!(permission_details(&json!({ "tool_name": "Read", "input": { "file_path": "D:/project/src/main.ts", "offset": 1 } })).is_some());
        assert!(permission_details(
            &json!({ "tool_name": "Bash", "input": { "command": "git status" } })
        )
        .is_none());
        assert!(permission_details(
            &json!({ "tool_name": "Read", "input": { "file_path": "x", "env": { "SECRET": "x" } } })
        )
        .is_none());
        let response = control_success("r", json!({ "behavior": "deny", "message": "declined" }));
        assert_eq!(response["response"]["request_id"], "r");
        assert_eq!(response["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn streaming_reads_text_delta_without_thought_or_tool_arguments() {
        let delta = json!({ "type": "stream_event", "event": { "type": "content_block_delta", "delta": { "type": "text_delta", "text": "hello" } } });
        assert_eq!(text_delta(&delta), Some("hello"));
        assert_eq!(
            text_delta(
                &json!({ "event": { "type": "content_block_delta", "delta": { "type": "input_json_delta", "partial_json": "secret" } } })
            ),
            None
        );
        assert_eq!(
            text_delta(
                &json!({ "event": { "type": "content_block_delta", "delta": { "type": "thinking_delta", "thinking": "hidden" } } })
            ),
            None
        );
    }

    #[test]
    fn native_activity_uses_ids_without_counting_every_background_process_as_subagent() {
        let mut subagents = std::collections::HashSet::new();
        let process = json!({"type":"system","subtype":"task_started","session_id":"s","task_id":"pid-job","task_type":"local_bash","description":"secret"});
        assert!(activities(&process, "s", "r", &mut subagents).is_empty());
        let agent = json!({"type":"system","subtype":"task_started","session_id":"s","task_id":"agent-1","task_type":"local_agent","description":"secret"});
        let events = activities(&agent, "s", "r", &mut subagents);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "subagent");
        assert!(!events[0].to_string().contains("secret"));
        assert!(activities(&agent, "other", "r", &mut subagents).is_empty());
        let finished = json!({"type":"system","subtype":"task_notification","session_id":"s","task_id":"agent-1","status":"completed","output_file":"secret"});
        assert_eq!(
            activities(&finished, "s", "r", &mut subagents)[0]["status"],
            "completed"
        );
        assert!(subagents.is_empty());
        assert!(activities(&finished, "s", "r", &mut subagents).is_empty());
    }

    #[test]
    fn streamed_and_complete_tool_frames_have_the_same_identity_and_results_close_it() {
        let mut subagents = std::collections::HashSet::new();
        let partial = json!({"type":"stream_event","session_id":"s","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"tool-1","name":"Read","input":{"file_path":"private"}}}});
        let full = json!({"type":"assistant","session_id":"s","message":{"content":[{"type":"tool_use","id":"tool-1","name":"Read","input":{"file_path":"private"}}]}});
        let first = activities(&partial, "s", "r", &mut subagents);
        let second = activities(&full, "s", "r", &mut subagents);
        assert_eq!(first[0]["id"], second[0]["id"]);
        assert!(!first[0].to_string().contains("private"));
        let result = json!({"type":"user","session_id":"s","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","is_error":true,"content":"credential"}]}});
        let events = activities(&result, "s", "r", &mut subagents);
        assert_eq!(events[0]["status"], "failed");
        assert!(!events[0].to_string().contains("credential"));
    }

    /// Opt-in compatibility probe only: no user frame or prompt is sent.
    /// The installed CLI may perform its own startup/auth metadata checks; this
    /// does not establish model-turn, login or permission-flow homologation.
    #[tokio::test]
    #[ignore = "explicit COUCOU_RUN_REAL_CLAUDE_HANDSHAKE=1 required; initializes the installed CLI without sending a prompt"]
    async fn real_claude_control_handshake_without_a_model_turn() {
        assert!(
            std::env::var("COUCOU_RUN_REAL_CLAUDE_HANDSHAKE").is_ok_and(|value| value == "1"),
            "Set COUCOU_RUN_REAL_CLAUDE_HANDSHAKE=1 explicitly to run the installed-CLI compatibility probe."
        );

        struct Workspace {
            base: std::path::PathBuf,
            directory: std::path::PathBuf,
        }
        impl Drop for Workspace {
            fn drop(&mut self) {
                if let Ok(target) = self.directory.canonicalize() {
                    if target.parent() == Some(self.base.as_path())
                        && target.file_name().is_some_and(|name| {
                            name.to_string_lossy()
                                .starts_with("coucou-claude-handshake-")
                        })
                    {
                        let _ = std::fs::remove_dir_all(target);
                    }
                }
            }
        }
        let base = std::env::temp_dir()
            .canonicalize()
            .expect("Cannot access the temporary directory.");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = base.join(format!(
            "coucou-claude-handshake-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).expect("Cannot create the isolated empty probe directory.");
        let workspace = Workspace { base, directory };
        let mut ctx = context();
        ctx.cwd = workspace.directory.clone();
        ctx.query.clear();
        let executable = super::super::process::resolve("claude").expect(
            "The official installed Claude executable is required for this explicit probe.",
        );
        let argv = args(&ctx).expect("Invalid consultative launch arguments.");
        let (_cancel, receiver) = tokio::sync::watch::channel(false);
        let sink: super::super::transport::Sink = std::sync::Arc::new(|_, _| {});
        let mut io = ProcessIo::spawn(
            &executable,
            &argv,
            &ctx,
            receiver,
            std::sync::Arc::new(super::super::Control::default()),
            sink,
        )
        .expect("Cannot start the installed Claude CLI for the isolated control handshake.");
        // initialize sends only control_request {subtype: initialize} and checks
        // the matching control_response success marker. Account/model catalogs
        // in the response are neither returned nor printed by this probe.
        let result = tokio::time::timeout(Duration::from_secs(20), initialize(&mut io)).await;
        io.shutdown().await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(_)) => panic!("The installed Claude CLI did not accept the consultative flags/control handshake. No prompt was sent."),
            Err(_) => panic!("The installed Claude control handshake timed out. No prompt was sent."),
        }
    }
}
