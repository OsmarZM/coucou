//! ACP v1 client for the locally installed Gemini and Copilot CLIs.
//! The subprocess owns its tools; disabled client capabilities are not an OS sandbox.

use std::time::{Duration, Instant};

use crate::usage::{activity, Collector};
use serde_json::{json, Value};

use super::{transport::ProcessIo, RunContext, RunOutcome};

const MAX_TEXT_BYTES: usize = 1_000_000;
const MAX_DETAIL_BYTES: usize = 16_000;

pub fn args(ctx: &RunContext, gemini_flag: &str) -> Vec<String> {
    if ctx.agent == "gemini" {
        vec![
            gemini_flag.to_string(),
            format!(
                "--approval-mode={}",
                if ctx.writable { "default" } else { "plan" }
            ),
        ]
    } else {
        let mut args = vec!["--acp".into(), "--stdio".into()];
        if !ctx.writable {
            args.push("--available-tools=view,glob,grep".into());
        }
        args
    }
}

pub async fn run(io: &mut ProcessIo, ctx: &RunContext) -> Result<RunOutcome, String> {
    let init = rpc(io, "initialize", json!({
        "protocolVersion": 1,
        "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
        "clientInfo": { "name": "coucou", "version": env!("CARGO_PKG_VERSION") }
    }), "coucou-acp-init").await?;
    if init.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
        return Err("The installed CLI uses an unsupported ACP protocol version.".into());
    }
    let params = json!({ "cwd": ctx.cwd.to_string_lossy(), "mcpServers": [] });
    let (session_id, session) = if let Some(id) = &ctx.session_id {
        if !valid_session_id(id) {
            return Err("Invalid ACP session ID. Start a new conversation.".into());
        }
        if init
            .pointer("/agentCapabilities/loadSession")
            .and_then(Value::as_bool)
            != Some(true)
        {
            return Err("This CLI cannot resume ACP sessions. Start a new conversation.".into());
        }
        let mut params = params;
        params["sessionId"] = json!(id);
        // Replayed history from session/load is deliberately discarded: the UI
        // already holds its transcript, and replay must not become new output.
        let session = rpc(io, "session/load", params, "coucou-acp-load").await?;
        (id.clone(), session)
    } else {
        let session = rpc(io, "session/new", params, "coucou-acp-new").await?;
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|id| valid_session_id(id))
            .ok_or("The CLI did not return a valid ACP session ID.")?
            .to_string();
        (id, session)
    };
    io.emit("session", json!({ "sessionId": session_id }));
    let mut usage = Collector::new(&ctx.agent, &session_id, &ctx.run_id);
    usage.version(init.pointer("/agentInfo/version").and_then(Value::as_str));
    if let Some(snapshot) = usage.take_update() {
        io.emit("usage", snapshot);
    }

    // Gemini's own plan mode restricts tools. Do not silently substitute an
    // implementation mode when an old CLI lacks it. Copilot is restricted at
    // launch by --available-tools in consultative conversations.
    if ctx.agent == "gemini" {
        let desired = if ctx.writable { "default" } else { "plan" };
        let mode = mode_id(&session, desired).ok_or(
            "The installed Gemini CLI does not advertise the required safe ACP mode. Update the CLI.",
        )?;
        rpc(
            io,
            "session/set_mode",
            json!({ "sessionId": session_id, "modeId": mode }),
            "coucou-acp-mode",
        )
        .await?;
    } else if let Some(mode) = mode_id(&session, "ask") {
        rpc(
            io,
            "session/set_mode",
            json!({ "sessionId": session_id, "modeId": mode }),
            "coucou-acp-mode",
        )
        .await?;
    }

    let prompt_id = format!("coucou-prompt-{}", ctx.run_id);
    io.send(
        json!({ "jsonrpc": "2.0", "id": prompt_id, "method": "session/prompt",
            "params": { "sessionId": session_id, "prompt": [{ "type": "text", "text": ctx.query }] }
        }),
    )
    .await?;
    let mut text = String::new();
    loop {
        let message = match io.next().await {
            Ok(message) => message,
            Err(_) if io.is_cancelled() => {
                let _ = io
                    .send(json!({ "jsonrpc": "2.0", "method": "session/cancel",
                    "params": { "sessionId": session_id } }))
                    .await;
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    cancellation_grace(io, &prompt_id),
                )
                .await;
                return Ok(RunOutcome {
                    session_id,
                    status: "interrupted".into(),
                    text,
                });
            }
            Err(err) => return Err(err),
        };
        if message.get("id") == Some(&json!(prompt_id)) && message.get("method").is_none() {
            let result = result(&message)?;
            usage.acp_result(&result);
            if let Some(snapshot) = usage.take_update() {
                io.emit("usage", snapshot);
            }
            let stop = result
                .get("stopReason")
                .and_then(Value::as_str)
                .ok_or("The CLI finished without an ACP stop reason.")?;
            let status = match stop {
                "cancelled" => "interrupted",
                "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" => "completed",
                _ => return Err("The CLI returned an unsupported ACP stop reason.".into()),
            };
            return Ok(RunOutcome {
                session_id,
                status: status.into(),
                text,
            });
        }
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            if message.get("id").is_some() {
                if method == "session/request_permission" {
                    permission(io, ctx, &session_id, &message).await?;
                } else {
                    unsupported(io, &message).await?;
                }
            } else if method == "session/update" {
                usage.acp_update(message.get("params").unwrap_or(&Value::Null));
                if let Some(snapshot) = usage.take_update() {
                    io.emit("usage", snapshot);
                }
                let Some(event) = update(&session_id, &message) else {
                    continue;
                };
                match event {
                    Update::Delta(delta) => {
                        append(&mut text, &delta)?;
                        io.emit("delta", json!({ "text": delta }));
                    }
                    Update::Tool { title, status, id } => {
                        if let Some(event) = activity(
                            &ctx.agent,
                            &session_id,
                            &ctx.run_id,
                            "tool",
                            &id,
                            &status,
                            "ACP/session/update.tool_call",
                        ) {
                            io.emit("activity", event);
                        }
                        io.emit(
                            "tool",
                            json!({ "text": title, "status": status, "toolCallId": id }),
                        );
                    }
                    Update::Mode(mode)
                        if ctx.agent == "gemini"
                            && mode != if ctx.writable { "default" } else { "plan" } =>
                    {
                        let _ = io.send(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": session_id } })).await;
                        return Err(
                            "The CLI changed its permission mode. The turn was stopped.".into()
                        );
                    }
                    Update::Mode(_) => {}
                }
            }
        }
    }
}

async fn rpc(io: &mut ProcessIo, method: &str, params: Value, id: &str) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(30), async {
        io.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await?;
        loop {
            let message = io.next().await?;
            if message.get("id") == Some(&json!(id)) && message.get("method").is_none() {
                return result(&message);
            }
            // Initialization/load cannot grant tools or execute client callbacks.
            // Old-session replay notifications are consumed without changing UI.
            if message.get("method").is_some() && message.get("id").is_some() {
                if message.get("method").and_then(Value::as_str)
                    == Some("session/request_permission")
                {
                    io.send(
                        json!({ "jsonrpc": "2.0", "id": message["id"], "result": cancelled() }),
                    )
                    .await?;
                } else {
                    unsupported(io, &message).await?;
                }
            }
        }
    })
    .await
    .map_err(|_| {
        "ACP session initialization timed out. Verify the CLI login and version.".to_string()
    })?
}

async fn cancellation_grace(io: &mut ProcessIo, prompt_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let Ok(message) = io
            .next_uncancelled(deadline.saturating_duration_since(Instant::now()))
            .await
        else {
            break;
        };
        if message.get("id") == Some(&json!(prompt_id)) && message.get("method").is_none() {
            break;
        }
        if message.get("method").is_some() && message.get("id").is_some() {
            if message.get("method").and_then(Value::as_str) == Some("session/request_permission") {
                let _ = io
                    .send(json!({ "jsonrpc": "2.0", "id": message["id"], "result": cancelled() }))
                    .await;
            } else {
                let _ = unsupported(io, &message).await;
            }
        }
    }
}

fn result(message: &Value) -> Result<Value, String> {
    if message.get("error").is_some() {
        // Protocol errors can include credentials or paths from provider config.
        let code = message.pointer("/error/code").and_then(Value::as_i64);
        return Err(match code {
            Some(-32000) => {
                "The CLI refused the request. Verify its login and project trust in the terminal."
            }
            Some(-32601) => "The installed CLI does not support this ACP method. Update the CLI.",
            _ => "The CLI returned an ACP error. Verify its login and version in the terminal.",
        }
        .into());
    }
    message
        .get("result")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or("The CLI returned an invalid ACP response.".into())
}

async fn unsupported(io: &mut ProcessIo, message: &Value) -> Result<(), String> {
    io.send(json!({ "jsonrpc": "2.0", "id": message["id"],
        "error": { "code": -32601, "message": "Client method not supported" }
    }))
    .await
}

async fn permission(
    io: &mut ProcessIo,
    ctx: &RunContext,
    session: &str,
    message: &Value,
) -> Result<(), String> {
    let params = &message["params"];
    let mut decision = "deny".to_string();
    let details = permission_details(params, session, ctx.writable);
    let options = params.get("options").and_then(Value::as_array);
    let has_once = options
        .and_then(|opts| option_id(opts, "allow_once"))
        .is_some();
    if has_once && !io.is_cancelled() {
        if let Some(details) = details {
            decision = io.approve(details).await.unwrap_or_else(|_| "deny".into());
        }
    }
    let outcome = permission_result(params, session, &decision, io.is_cancelled());
    io.send(json!({ "jsonrpc": "2.0", "id": message["id"], "result": outcome }))
        .await
}

fn cancelled() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

fn permission_result(params: &Value, session: &str, decision: &str, cancelled_turn: bool) -> Value {
    if cancelled_turn || params.get("sessionId").and_then(Value::as_str) != Some(session) {
        return cancelled();
    }
    let Some(options) = params.get("options").and_then(Value::as_array) else {
        return cancelled();
    };
    let kind = if decision == "allow" {
        "allow_once"
    } else {
        "reject_once"
    };
    match option_id(options, kind) {
        Some(id) => json!({ "outcome": { "outcome": "selected", "optionId": id } }),
        None => cancelled(),
    }
}

fn option_id<'a>(options: &'a [Value], kind: &str) -> Option<&'a str> {
    options.iter().find_map(|option| {
        if option.get("kind").and_then(Value::as_str) != Some(kind) {
            return None;
        }
        option
            .get("optionId")
            .and_then(Value::as_str)
            .filter(|id| valid_session_id(id))
    })
}

fn permission_details(params: &Value, session: &str, writable: bool) -> Option<Value> {
    if params.get("sessionId").and_then(Value::as_str) != Some(session) {
        return None;
    }
    let call = params.get("toolCall")?;
    let kind = call.get("kind").and_then(Value::as_str).unwrap_or("other");
    if kind == "switch_mode" || (!writable && !matches!(kind, "read" | "search" | "think")) {
        return None;
    }
    let title = call
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 300)?;
    if sensitive(title) {
        return None;
    }
    let input = call.get("rawInput").filter(|v| !v.is_null());
    // Never approve an opaque writable operation based only on a friendly title.
    let projected = match input {
        Some(input) => inspectable_input(input)?,
        // Gemini's ACP adapter puts proposed edits in diff content rather than
        // rawInput. Show the complete diff and target instead of denying every
        // edit, and never treat a generic explanation as an executable input.
        None => inspectable_content(call, kind, writable)?,
    };
    let detail = serde_json::to_string_pretty(&projected).ok()?;
    if detail.len() > MAX_DETAIL_BYTES || sensitive(&detail) {
        return None;
    }
    Some(json!({ "title": title, "detail": detail, "sessionId": session }))
}

fn inspectable_input(input: &Value) -> Option<Value> {
    // A full known input is displayed, never a projection that hides side effects.
    let object = input.as_object()?;
    const FIELDS: &[&str] = &[
        "command",
        "commands",
        "path",
        "file_path",
        "absolute_path",
        "pattern",
        "query",
        "content",
        "old_string",
        "new_string",
        "oldText",
        "newText",
        "replace_all",
        "description",
        "timeout",
        "dir_path",
        "include",
        "exclude",
        "case_sensitive",
        "limit",
        "offset",
    ];
    if object.is_empty() || object.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return None;
    }
    if object.values().any(|v| {
        !(v.is_string()
            || v.is_boolean()
            || v.is_number()
            || v.is_null()
            || v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
    }) {
        return None;
    }
    Some(input.clone())
}

fn inspectable_content(call: &Value, kind: &str, writable: bool) -> Option<Value> {
    let content = call.get("content").and_then(Value::as_array)?;
    let mut details = Vec::new();
    let mut has_diff = false;
    for block in content {
        match block.get("type").and_then(Value::as_str)? {
            "diff" if writable && matches!(kind, "edit" | "delete" | "move") => {
                let path = block.get("path")?.as_str()?.to_string();
                let old = block.get("oldText").cloned().unwrap_or(Value::Null);
                if !(old.is_null() || old.is_string()) {
                    return None;
                }
                let new = block.get("newText")?.as_str()?;
                details.push(json!({ "path": path, "oldText": old, "newText": new }));
                has_diff = true;
            }
            "content" => {
                let body = block.get("content")?;
                if body.get("type").and_then(Value::as_str) != Some("text") {
                    return None;
                }
                details.push(json!({ "description": body.get("text")?.as_str()? }));
            }
            _ => return None,
        }
    }
    if writable && !has_diff {
        return None;
    }
    let locations = match call.get("locations").and_then(Value::as_array) {
        Some(locations) => Some(locations.iter().map(|location| {
            let path = location.get("path")?.as_str()?;
            Some(json!({ "path": path, "line": location.get("line").and_then(Value::as_u64) }))
        }).collect::<Option<Vec<_>>>()?),
        None => None,
    };
    if !has_diff && details.is_empty() && locations.as_ref().is_none_or(Vec::is_empty) {
        return None;
    }
    Some(json!({ "kind": kind, "content": details, "locations": locations }))
}

fn sensitive(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "authorization",
        "bearer ",
        "password",
        "api_key",
        "apikey",
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
}

fn mode_id(session: &Value, wanted: &str) -> Option<String> {
    session
        .pointer("/modes/availableModes")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|mode| {
            mode.get("id")
                .and_then(Value::as_str)
                .filter(|id| *id == wanted)
                .map(str::to_string)
        })
}

fn valid_session_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}

#[derive(Debug, PartialEq)]
enum Update {
    Delta(String),
    Tool {
        title: String,
        status: String,
        id: String,
    },
    Mode(String),
}

fn update(session: &str, message: &Value) -> Option<Update> {
    let params = message.get("params")?;
    if params.get("sessionId").and_then(Value::as_str) != Some(session) {
        return None;
    }
    let update = params.get("update")?;
    match update.get("sessionUpdate").and_then(Value::as_str)? {
        "agent_message_chunk" => {
            let content = update.get("content")?;
            if content.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            Some(Update::Delta(content.get("text")?.as_str()?.to_string()))
        }
        "tool_call" | "tool_call_update" => {
            let title = update
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Agent tool activity");
            let title = if sensitive(title) {
                "Agent tool activity"
            } else {
                title
            };
            Some(Update::Tool {
                title: title.chars().take(300).collect(),
                status: update
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending")
                    .chars()
                    .take(40)
                    .collect(),
                id: update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(256)
                    .collect(),
            })
        }
        "current_mode_update" => Some(Update::Mode(
            update.get("currentModeId")?.as_str()?.to_string(),
        )),
        // Thought chunks, raw tool output, filesystem data and usage stay out
        // of chat. User replay is not an assistant answer.
        _ => None,
    }
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

    #[test]
    fn stream_is_scoped_and_only_assistant_text_is_visible() {
        let message = json!({ "params": { "sessionId": "a", "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "hello" } } } });
        assert_eq!(update("a", &message), Some(Update::Delta("hello".into())));
        assert_eq!(update("b", &message), None);
        let thought = json!({ "params": { "sessionId": "a", "update": { "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "hidden" } } } });
        assert_eq!(update("a", &thought), None);
    }

    #[test]
    fn permission_never_substitutes_an_always_option_or_another_session() {
        let params = json!({ "sessionId": "s", "options": [
            { "kind": "allow_always", "optionId": "forever" },
            { "kind": "allow_once", "optionId": "one" },
            { "kind": "reject_once", "optionId": "no" }
        ] });
        assert_eq!(
            permission_result(&params, "s", "allow", false)["outcome"]["optionId"],
            "one"
        );
        assert_eq!(
            permission_result(&params, "s", "deny", false)["outcome"]["optionId"],
            "no"
        );
        assert_eq!(
            permission_result(&params, "other", "allow", false),
            cancelled()
        );
        assert_eq!(permission_result(&params, "s", "allow", true), cancelled());
        assert_eq!(
            permission_result(
                &json!({ "sessionId": "s", "options": [{ "kind": "allow_always", "optionId": "forever" }] }),
                "s",
                "allow",
                false
            ),
            cancelled()
        );
    }

    #[test]
    fn opaque_sensitive_or_write_actions_are_not_approvable_in_consultative_chat() {
        let base = json!({ "sessionId": "s", "toolCall": { "kind": "execute", "title": "Run test", "rawInput": { "command": "npm test" } } });
        assert!(permission_details(&base, "s", false).is_none());
        assert!(permission_details(&base, "s", true).is_some());
        let sensitive = json!({ "sessionId": "s", "toolCall": { "kind": "execute", "title": "Login", "rawInput": { "command": "curl -H 'Authorization: bearer token'" } } });
        assert!(permission_details(&sensitive, "s", true).is_none());
        let opaque = json!({ "sessionId": "s", "toolCall": { "kind": "execute", "title": "Run test", "rawInput": { "environment": { "SECRET": "x" }, "command": "npm test" } } });
        assert!(permission_details(&opaque, "s", true).is_none());
        assert!(permission_details(
            &json!({ "sessionId": "s", "toolCall": { "kind": "edit", "title": "Change file" } }),
            "s",
            true
        )
        .is_none());
    }

    #[test]
    fn protocol_errors_do_not_echo_provider_credentials() {
        let error = result(&json!({ "error": { "code": -32000, "message": "sk-ant-secret" } }))
            .unwrap_err();
        assert!(!error.contains("sk-ant"));
        assert!(result(&json!({ "result": [] })).is_err());
    }

    #[test]
    fn gemini_diff_approval_contains_full_edit_and_rejects_large_or_secret_content() {
        let mut params = json!({ "sessionId": "s", "toolCall": { "kind": "edit", "title": "Edit main.ts", "content": [{ "type": "diff", "path": "D:/project/main.ts", "oldText": "before", "newText": "after" }], "locations": [{ "path": "D:/project/main.ts", "line": 1 }] } });
        let details = permission_details(&params, "s", true).unwrap();
        let text = details["detail"].as_str().unwrap();
        assert!(text.contains("before") && text.contains("after") && text.contains("main.ts"));
        assert!(permission_details(&params, "s", false).is_none());
        params["toolCall"]["content"][0]["newText"] = json!("x".repeat(16_001));
        assert!(permission_details(&params, "s", true).is_none());
        params["toolCall"]["content"][0]["newText"] = json!("api_key=secret");
        assert!(permission_details(&params, "s", true).is_none());
    }

    #[test]
    fn launch_arguments_enforce_native_consultative_modes_without_auto_grants() {
        let mut ctx = RunContext {
            conversation_id: "c".into(),
            run_id: "r".into(),
            agent: "gemini".into(),
            cwd: std::path::PathBuf::from("D:/project"),
            session_id: None,
            query: "hello".into(),
            writable: false,
            personal: false,
            context_id: None,
        };
        assert_eq!(args(&ctx, "--acp"), ["--acp", "--approval-mode=plan"]);
        ctx.writable = true;
        assert_eq!(
            args(&ctx, "--experimental-acp"),
            ["--experimental-acp", "--approval-mode=default"]
        );
        ctx.agent = "copilot".into();
        ctx.writable = false;
        assert_eq!(
            args(&ctx, "--acp"),
            ["--acp", "--stdio", "--available-tools=view,glob,grep"]
        );
        ctx.writable = true;
        assert_eq!(args(&ctx, "--acp"), ["--acp", "--stdio"]);
    }
}
