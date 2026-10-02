//! The wire contract shared by the hook relay and the Windows app.
//!
//! Native hook input never crosses the pipe. Each adapter builds a small
//! allowlisted event so prompts, transcripts, patches, environment and tool
//! output stay in the CLI process. The API deliberately has no storage or IPC
//! dependency; it is also used to normalize legacy Claude relays in the app.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_METADATA_LEN: usize = 800;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_ID_LEN: usize = 256;
const MAX_CWD_LEN: usize = 4096;
const OMITTED: &str = "[sensitive metadata omitted]";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Gemini,
    Copilot,
}

impl Agent {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "copilot" => Some(Self::Copilot),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Copilot => "copilot",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentEventType {
    SessionStarted,
    TurnStarted,
    ToolStarted,
    ToolFinished,
    ToolFailed,
    ApprovalRequested,
    Notification,
    TurnFinished,
    TurnFailed,
    SessionEnded,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentEvent {
    pub protocol_version: u8,
    pub agent: Agent,
    pub session_id: String,
    pub event_type: AgentEventType,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_target: Option<String>,
    #[serde(default)]
    pub requires_approval: bool,
    /// Assigned by the backend, never copied from native stdin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolError(pub &'static str);

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ProtocolError {}

impl AgentEvent {
    /// JSON tuple encoding keeps `(agent, session_id)` unambiguous even when a
    /// native ID contains the separators another session happens to use.
    pub fn session_key(&self) -> String {
        serde_json::to_string(&(self.agent, &self.session_id)).unwrap_or_default()
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError("unsupported protocol version"));
        }
        validate_id(&self.session_id)?;
        if let Some(id) = &self.turn_id {
            validate_id(id)?;
        }
        if let Some(id) = &self.request_id {
            validate_id(id)?;
        }
        if self.cwd.len() > MAX_CWD_LEN || self.cwd.chars().any(char::is_control) {
            return Err(ProtocolError("invalid cwd"));
        }
        for text in [&self.tool_name, &self.summary, &self.approval_target]
            .into_iter()
            .flatten()
        {
            if text.len() > MAX_METADATA_LEN || text.chars().any(char::is_control) {
                return Err(ProtocolError("invalid metadata"));
            }
            if sanitize_metadata(text) != *text {
                return Err(ProtocolError("unsanitized metadata"));
            }
        }
        if self.source.as_deref().is_some_and(|source| source != "cli") {
            return Err(ProtocolError("unsupported source"));
        }
        if self.requires_approval
            && (self.agent != Agent::Claude
                || self.event_type != AgentEventType::ApprovalRequested
                || self.approval_target.as_deref().is_none_or(|target| {
                    target.trim().is_empty() || target.contains(OMITTED) || target.ends_with('…')
                }))
        {
            return Err(ProtocolError(
                "interactive approval is unavailable for this event",
            ));
        }
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), ProtocolError> {
    if id.trim().is_empty()
        || id.len() > MAX_ID_LEN
        || id.trim() != id
        || id.chars().any(char::is_control)
    {
        return Err(ProtocolError(
            "missing or invalid native session/turn/request id",
        ));
    }
    Ok(())
}

/// Conservative metadata cleanup without another dependency. Sensitive
/// assignments and authenticated URLs are omitted as a whole rather than
/// retaining an untested part of a secret. ANSI and other control characters
/// are never allowed to influence the island's rendering.
pub fn sanitize_metadata(input: &str) -> String {
    let mut clean = String::with_capacity(input.len().min(MAX_METADATA_LEN));
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    for ansi in chars.by_ref() {
                        if ('@'..='~').contains(&ansi) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(ansi) = chars.next() {
                        if ansi == '\u{7}' || (ansi == '\u{1b}' && chars.next() == Some('\\')) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !c.is_control() {
            clean.push(c);
        } else if c == '\n' || c == '\r' || c == '\t' {
            clean.push(' ');
        }
    }
    let clean = clean.trim();
    let lower = clean.to_ascii_lowercase();
    if [
        "://", "bearer ", "basic ", "sk-", "dk-", "ghp_", "github_pat_", "authorization:",
        "--password", "--token", "--secret", "--api-key", "--apikey", "--prompt",
        "password:", "secret:", "token:", "api_key:", "api-key:", "apikey:", "database_url:",
    ].iter().any(|pattern| lower.contains(pattern))
        // A command containing an assignment may expose environment or content,
        // including variable names that do not advertise their sensitivity.
        || clean.contains('=')
    {
        return OMITTED.to_owned();
    }
    if clean.len() <= MAX_METADATA_LEN {
        return clean.to_owned();
    }
    let mut end = MAX_METADATA_LEN - '…'.len_utf8();
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &clean[..end])
}

fn native_string<'a>(input: &'a Value, native: &str, camel: &str) -> Option<&'a str> {
    input
        .get(native)
        .or_else(|| input.get(camel))
        .and_then(Value::as_str)
}

/// `event_name` comes from the hook command, including Copilot's camelCase
/// hooks, whose native payload does not carry an event name. A conflicting
/// payload event is rejected instead of being promoted to an approval.
pub fn normalize(
    agent: Agent,
    event_name: &str,
    input: &Value,
) -> Result<Option<AgentEvent>, ProtocolError> {
    if !input.is_object() {
        return Err(ProtocolError("hook input must be a JSON object"));
    }
    let supplied = input
        .get("hook_event_name")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty());
    let hook = if event_name.is_empty() {
        supplied.unwrap_or("")
    } else {
        event_name
    };
    if agent != Agent::Copilot && supplied.is_some_and(|event| event != hook) {
        return Err(ProtocolError("hook event does not match command"));
    }
    let event_type = match (agent, hook) {
        (Agent::Claude | Agent::Codex | Agent::Gemini, "SessionStart")
        | (Agent::Copilot, "sessionStart") => AgentEventType::SessionStarted,
        (Agent::Claude | Agent::Codex | Agent::Gemini, "SessionEnd")
        | (Agent::Copilot, "sessionEnd") => AgentEventType::SessionEnded,
        (Agent::Claude | Agent::Codex, "UserPromptSubmit")
        | (Agent::Gemini, "BeforeAgent")
        | (Agent::Copilot, "userPromptSubmitted") => AgentEventType::TurnStarted,
        (Agent::Claude | Agent::Codex, "PreToolUse")
        | (Agent::Gemini, "BeforeTool")
        | (Agent::Copilot, "preToolUse") => AgentEventType::ToolStarted,
        (Agent::Claude | Agent::Codex, "PostToolUse") | (Agent::Copilot, "postToolUse") => {
            AgentEventType::ToolFinished
        }
        (Agent::Gemini, "AfterTool") => {
            // A documented structured error is evidence of tool failure. We do
            // not infer failure or success from arbitrary model/tool output.
            if input
                .get("tool_response")
                .and_then(|v| v.get("error"))
                .is_some_and(|error| !error.is_null() && error != false)
            {
                AgentEventType::ToolFailed
            } else {
                AgentEventType::ToolFinished
            }
        }
        (Agent::Claude, "PostToolUseFailure") | (Agent::Copilot, "postToolUseFailure") => {
            AgentEventType::ToolFailed
        }
        (Agent::Claude | Agent::Codex, "PermissionRequest")
        | (Agent::Copilot, "permissionRequest") => AgentEventType::ApprovalRequested,
        (Agent::Claude | Agent::Gemini, "Notification") => {
            if input.get("notification_type").and_then(Value::as_str) == Some("permission_prompt")
                || (agent == Agent::Gemini
                    && input.get("notification_type").and_then(Value::as_str)
                        == Some("ToolPermission"))
            {
                AgentEventType::ApprovalRequested
            } else {
                AgentEventType::Notification
            }
        }
        (Agent::Copilot, "notification" | "preCompact") => AgentEventType::Notification,
        (Agent::Copilot, "errorOccurred") => {
            if input.get("recoverable").and_then(Value::as_bool) == Some(false) {
                AgentEventType::TurnFailed
            } else {
                AgentEventType::Notification
            }
        }
        (Agent::Claude | Agent::Codex, "Stop")
        | (Agent::Gemini, "AfterAgent")
        | (Agent::Copilot, "agentStop") => AgentEventType::TurnFinished,
        (Agent::Claude, "StopFailure") => AgentEventType::TurnFailed,
        (Agent::Codex, "Interrupt") => AgentEventType::Interrupted,
        // In particular, SubagentStop must not finish its parent's turn.
        _ => return Ok(None),
    };

    let session_id = if agent == Agent::Copilot {
        native_string(input, "sessionId", "session_id")
    } else {
        input.get("session_id").and_then(Value::as_str)
    }
    .unwrap_or("");
    validate_id(session_id)?;
    let cwd = input
        .get("cwd")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let raw_tool = native_string(input, "tool_name", "toolName");
    let tool_name = raw_tool
        .map(sanitize_metadata)
        .filter(|tool| !tool.is_empty());
    let tool_input = input
        .get("tool_input")
        .or_else(|| input.get("toolArgs"))
        .or_else(|| input.get("toolInput"));
    let parsed_args;
    let args = match tool_input {
        Some(Value::String(raw)) => {
            parsed_args = serde_json::from_str::<Value>(raw).unwrap_or(Value::Null);
            &parsed_args
        }
        Some(value) => value,
        None => &Value::Null,
    };
    let path = args
        .get("file_path")
        .or_else(|| args.get("path"))
        .and_then(Value::as_str);
    // `command` may contain a patch or a full file. Only tools explicitly
    // identified as shells can expose a short command as activity metadata.
    let is_shell = matches!(
        raw_tool,
        Some("Bash" | "PowerShell" | "bash" | "powershell" | "shell" | "run_shell_command")
    );
    let command = is_shell
        .then(|| args.get("command").and_then(Value::as_str))
        .flatten();
    let detail = command.or(path);
    let detail_is_reviewable = detail.is_some_and(|text| {
        !text.trim().is_empty()
            && text.len() <= MAX_METADATA_LEN / 2
            && !text.chars().any(char::is_control)
            && sanitize_metadata(text) == text.trim()
    });
    let summary = match event_type {
        AgentEventType::ToolStarted
        | AgentEventType::ToolFinished
        | AgentEventType::ToolFailed
        | AgentEventType::ApprovalRequested => {
            let tool = tool_name.as_deref().unwrap_or("Tool");
            let mut summary = tool.to_owned();
            if let Some(detail) = detail {
                // Multiline input and large commands can embed file/prompt
                // bodies. Their contents are omitted even after truncation.
                let safe = if detail.len() <= MAX_METADATA_LEN / 2
                    && !detail.chars().any(char::is_control)
                {
                    sanitize_metadata(detail)
                } else {
                    OMITTED.to_owned()
                };
                summary = format!("{tool} · {safe}");
            }
            Some(sanitize_metadata(&summary))
        }
        AgentEventType::Notification => Some(if hook == "errorOccurred" {
            "Recoverable agent error".to_owned()
        } else if hook == "preCompact" {
            "Compacting context".to_owned()
        } else if input.get("notification_type").and_then(Value::as_str) == Some("idle_prompt") {
            "Waiting for input".to_owned()
        } else {
            "Agent notification".to_owned()
        }),
        AgentEventType::TurnFailed => Some("Agent turn failed".to_owned()),
        _ => None,
    };
    let approval_target = (event_type == AgentEventType::ApprovalRequested && detail_is_reviewable)
        .then(|| summary.clone())
        .flatten()
        .filter(|target| !target.contains(OMITTED) && !target.ends_with('…'));
    let requires_approval =
        agent == Agent::Claude && hook == "PermissionRequest" && approval_target.is_some();
    let event = AgentEvent {
        protocol_version: PROTOCOL_VERSION,
        agent,
        session_id: session_id.to_owned(),
        event_type,
        cwd,
        tool_name,
        summary,
        turn_id: native_string(input, "turn_id", "turnId").map(str::to_owned),
        approval_target,
        requires_approval,
        request_id: None,
        source: Some("cli".to_owned()),
    };
    event.validate()?;
    Ok(Some(event))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input() -> Value {
        json!({"session_id":"same-session", "cwd":"C:\\work"})
    }

    #[test]
    fn identity_separates_agent_and_native_session_without_cwd_fallback() {
        let claude = normalize(Agent::Claude, "SessionStart", &input())
            .unwrap()
            .unwrap();
        let codex = normalize(Agent::Codex, "SessionStart", &input())
            .unwrap()
            .unwrap();
        assert_ne!(claude.session_key(), codex.session_key());
        let other = normalize(
            Agent::Claude,
            "SessionStart",
            &json!({
                "session_id":"same-session:other", "cwd":"C:\\work"
            }),
        )
        .unwrap()
        .unwrap();
        assert_ne!(claude.session_key(), other.session_key());
        for bad in [
            json!({"cwd":"C:\\work"}),
            json!({"session_id":""}),
            json!({"session_id":" \t "}),
            json!({"session_id":"a\u{0}b"}),
            json!({"session_id":"x".repeat(257)}),
        ] {
            assert!(normalize(Agent::Claude, "SessionStart", &bad).is_err());
        }
    }

    #[test]
    fn turns_sessions_and_subagents_are_distinct() {
        for (agent, stop, end) in [
            (Agent::Claude, "Stop", "SessionEnd"),
            (Agent::Codex, "Stop", "SessionEnd"),
            (Agent::Gemini, "AfterAgent", "SessionEnd"),
        ] {
            assert_eq!(
                normalize(agent, stop, &input())
                    .unwrap()
                    .unwrap()
                    .event_type,
                AgentEventType::TurnFinished
            );
            assert_eq!(
                normalize(agent, end, &input()).unwrap().unwrap().event_type,
                AgentEventType::SessionEnded
            );
            assert!(normalize(agent, "SubagentStop", &input())
                .unwrap()
                .is_none());
        }
        assert_eq!(
            normalize(Agent::Codex, "Interrupt", &input())
                .unwrap()
                .unwrap()
                .event_type,
            AgentEventType::Interrupted
        );
    }

    #[test]
    fn all_agents_turn_start_without_forwarding_prompts() {
        for (agent, hook, session) in [
            (Agent::Claude, "UserPromptSubmit", "session_id"),
            (Agent::Codex, "UserPromptSubmit", "session_id"),
            (Agent::Gemini, "BeforeAgent", "session_id"),
            (Agent::Copilot, "userPromptSubmitted", "sessionId"),
        ] {
            let mut payload = json!({"prompt":"DO NOT FORWARD", "env":{"TOKEN":"PRIVATE"},
                "transcript_path":"PRIVATE", "initialPrompt":"PRIVATE", "requestId":"FORGED"});
            payload[session] = json!("s");
            let event = normalize(agent, hook, &payload).unwrap().unwrap();
            assert_eq!(event.event_type, AgentEventType::TurnStarted);
            let serialized = serde_json::to_string(&event).unwrap();
            for forbidden in [
                "DO NOT FORWARD",
                "PRIVATE",
                "FORGED",
                "prompt",
                "transcript",
                "env",
            ] {
                assert!(!serialized.contains(forbidden), "leaked {forbidden}");
            }
        }
    }

    #[test]
    fn copilot_native_event_and_string_arguments_are_supported() {
        let event = normalize(
            Agent::Copilot,
            "preToolUse",
            &json!({
                "sessionId":"c", "toolName":"powershell", "toolArgs":"{\"command\":\"cargo test\"}"
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(event.event_type, AgentEventType::ToolStarted);
        assert_eq!(event.summary.as_deref(), Some("powershell · cargo test"));
        let failed = normalize(
            Agent::Copilot,
            "postToolUseFailure",
            &json!({
                "sessionId":"c", "toolName":"bash", "error":"PRIVATE OUTPUT"
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(failed.event_type, AgentEventType::ToolFailed);
        assert!(!serde_json::to_string(&failed).unwrap().contains("PRIVATE"));
    }

    #[test]
    fn documented_failure_evidence_does_not_scrape_arbitrary_tool_output() {
        let codex = normalize(
            Agent::Codex,
            "PostToolUse",
            &json!({
                "session_id":"s", "tool_response":"Process exited 1 SECRET"
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(codex.event_type, AgentEventType::ToolFinished);
        assert!(!serde_json::to_string(&codex).unwrap().contains("SECRET"));
        let gemini = normalize(
            Agent::Gemini,
            "AfterTool",
            &json!({
                "session_id":"s", "tool_response":{"error":{"message":"PRIVATE"}}
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(gemini.event_type, AgentEventType::ToolFailed);
        let claude = normalize(Agent::Claude, "PostToolUseFailure", &input())
            .unwrap()
            .unwrap();
        assert_eq!(claude.event_type, AgentEventType::ToolFailed);
        assert!(normalize(Agent::Codex, "PostToolUseFailure", &input())
            .unwrap()
            .is_none());
    }

    #[test]
    fn patch_file_content_and_multiline_commands_never_enter_metadata() {
        for (agent, hook, payload) in [
            (
                Agent::Codex,
                "PreToolUse",
                json!({"session_id":"s", "tool_name":"apply_patch",
                "tool_input":{"command":"*** Begin Patch\nPRIVATE\n*** End Patch"}}),
            ),
            (
                Agent::Claude,
                "PreToolUse",
                json!({"session_id":"s", "tool_name":"Write",
                "tool_input":{"file_path":"C:\\foo.rs", "content":"PRIVATE", "command":"PRIVATE"}}),
            ),
            (
                Agent::Gemini,
                "BeforeTool",
                json!({"session_id":"s", "tool_name":"run_shell_command",
                "tool_input":{"command":"echo PRIVATE\nline 2"}}),
            ),
            (
                Agent::Copilot,
                "preToolUse",
                json!({"sessionId":"s", "toolName":"create",
                "toolArgs":{"path":"C:\\foo.rs", "command":"PRIVATE", "fileText":"PRIVATE"}}),
            ),
        ] {
            let event = normalize(agent, hook, &payload).unwrap().unwrap();
            assert!(!serde_json::to_string(&event).unwrap().contains("PRIVATE"));
        }
    }

    #[test]
    fn metadata_removes_sensitive_values_and_ansi_before_display() {
        for secret in [
            "TOKEN=secret",
            "ENV_NAME=unknown",
            "Bearer secret",
            "Basic secret",
            "https://user:secret@host/path",
            "github_pat_secret123",
            "ghp_secret123",
            "sk-secret123",
            "--prompt 'private'",
            "--password private",
            "api_key: private",
            "password: private",
        ] {
            assert_eq!(sanitize_metadata(secret), OMITTED);
        }
        assert_eq!(sanitize_metadata("\u{1b}[31mOlá\u{1b}[0m\0"), "Olá");
        assert_eq!(
            sanitize_metadata("\u{1b}]0;PRIVATE\u{7}cargo test"),
            "cargo test"
        );
        let long = sanitize_metadata(&"🦉".repeat(900));
        assert!(long.len() <= MAX_METADATA_LEN);
        assert!(long.ends_with('…'));
        assert_eq!(sanitize_metadata(&long), long);
    }

    #[test]
    fn interactive_approval_is_exclusively_inspectable_claude_permission_request() {
        let payload = json!({"session_id":"s", "tool_name":"Bash",
            "tool_input":{"command":"cargo test"}, "request_id":"forged"});
        let claude = normalize(Agent::Claude, "PermissionRequest", &payload)
            .unwrap()
            .unwrap();
        assert!(claude.requires_approval);
        assert_eq!(claude.approval_target.as_deref(), Some("Bash · cargo test"));
        assert!(claude.request_id.is_none());
        let codex = normalize(Agent::Codex, "PermissionRequest", &payload)
            .unwrap()
            .unwrap();
        assert!(!codex.requires_approval);
        let copilot = normalize(
            Agent::Copilot,
            "permissionRequest",
            &json!({
                "sessionId":"s", "toolName":"bash", "toolArgs":{"command":"cargo test"}
            }),
        )
        .unwrap()
        .unwrap();
        assert!(!copilot.requires_approval);
        let gemini = normalize(
            Agent::Gemini,
            "Notification",
            &json!({
                "session_id":"s", "notification_type":"ToolPermission", "message":"PRIVATE"
            }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(gemini.event_type, AgentEventType::ApprovalRequested);
        assert!(!gemini.requires_approval);
        assert!(!serde_json::to_string(&gemini).unwrap().contains("PRIVATE"));
    }

    #[test]
    fn missing_sensitive_or_truncated_claude_targets_defer_to_native_permission() {
        for args in [
            json!({}),
            json!({"prompt":"PRIVATE"}),
            json!({"command":"TOKEN=PRIVATE cargo test"}),
            json!({"command":"x".repeat(1000)}),
            json!({"command":"cargo test\necho PRIVATE"}),
        ] {
            let event = normalize(
                Agent::Claude,
                "PermissionRequest",
                &json!({
                    "session_id":"s", "tool_name":"Bash", "tool_input":args
                }),
            )
            .unwrap()
            .unwrap();
            assert!(!event.requires_approval);
            assert!(event.approval_target.is_none());
            assert!(!serde_json::to_string(&event).unwrap().contains("PRIVATE"));
        }
    }

    #[test]
    fn validation_rejects_forged_protocol_approvals_and_extra_native_fields() {
        let mut event = normalize(Agent::Codex, "SessionStart", &input())
            .unwrap()
            .unwrap();
        event.requires_approval = true;
        assert!(event.validate().is_err());
        event.requires_approval = false;
        event.protocol_version = 2;
        assert!(event.validate().is_err());
        event.protocol_version = PROTOCOL_VERSION;
        event.summary = Some("TOKEN=PRIVATE".into());
        assert!(event.validate().is_err());
        let mut serialized = serde_json::to_value(
            normalize(Agent::Claude, "SessionStart", &input())
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        serialized["prompt"] = json!("PRIVATE");
        assert!(serde_json::from_value::<AgentEvent>(serialized).is_err());
    }

    #[test]
    fn event_domain_and_argument_mismatch_are_rejected() {
        assert!(normalize(Agent::Gemini, "PreToolUse", &input())
            .unwrap()
            .is_none());
        assert!(normalize(Agent::Codex, "Notification", &input())
            .unwrap()
            .is_none());
        assert!(normalize(
            Agent::Claude,
            "PermissionRequest",
            &json!({
                "session_id":"s", "hook_event_name":"SessionEnd"
            })
        )
        .is_err());
        assert!(normalize(Agent::Claude, "SessionStart", &json!([])).is_err());
        assert!(normalize(
            Agent::Claude,
            "SessionStart",
            &json!({
                "session_id":"s", "turn_id":"bad\u{0}id"
            })
        )
        .is_err());
    }

    #[test]
    fn wire_contract_uses_camel_case_and_optional_native_turn_ids() {
        let event = normalize(
            Agent::Codex,
            "Stop",
            &json!({
                "session_id":"s", "turn_id":"turn-1"
            }),
        )
        .unwrap()
        .unwrap();
        let encoded = serde_json::to_value(&event).unwrap();
        assert_eq!(encoded["protocolVersion"], 1);
        assert_eq!(encoded["agent"], "codex");
        assert_eq!(encoded["sessionId"], "s");
        assert_eq!(encoded["turnId"], "turn-1");
        assert_eq!(encoded["eventType"], "turnFinished");
        assert_eq!(encoded["requiresApproval"], false);
        assert_eq!(encoded["source"], "cli");
        assert_eq!(
            serde_json::from_value::<AgentEvent>(encoded).unwrap(),
            event
        );
    }
}
