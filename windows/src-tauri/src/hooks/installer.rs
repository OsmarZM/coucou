//! Provider-specific, reversible hook configuration. No CLI is run by this module.
//! Preview and apply share one snapshot; apply refuses stale or malformed files.

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::settings;

const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;
const MARKER: &str = "--coucou-managed-v1";
const CODEX_LAUNCHER: &str = "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"& '";

pub const HOOK_EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 10),
    ("SessionEnd", 10),
    ("UserPromptSubmit", 10),
    ("PreToolUse", 10),
    ("PostToolUse", 10),
    ("PostToolUseFailure", 10),
    ("PermissionRequest", 120),
    ("Notification", 10),
    ("Stop", 10),
    ("StopFailure", 10),
    ("SubagentStart", 10),
    ("SubagentStop", 10),
];
const CODEX_EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 10),
    ("SessionEnd", 3),
    ("UserPromptSubmit", 10),
    ("PreToolUse", 10),
    ("PostToolUse", 10),
    ("Stop", 10),
    ("Interrupt", 3),
];
const GEMINI_EVENTS: &[(&str, u64)] = &[
    ("SessionStart", 10000),
    ("SessionEnd", 10000),
    ("BeforeAgent", 10000),
    ("AfterAgent", 10000),
    ("BeforeTool", 10000),
    ("AfterTool", 10000),
    ("Notification", 10000),
];
const COPILOT_EVENTS: &[(&str, u64)] = &[
    ("sessionStart", 10),
    ("sessionEnd", 10),
    ("userPromptSubmitted", 10),
    ("preToolUse", 10),
    ("postToolUse", 10),
    ("postToolUseFailure", 10),
    ("agentStop", 10),
    ("notification", 10),
    ("errorOccurred", 10),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Agent {
    Claude,
    Codex,
    Gemini,
    Copilot,
}

impl Agent {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "copilot" => Ok(Self::Copilot),
            _ => Err(format!("Unsupported agent: {name}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Copilot => "copilot",
        }
    }
    fn events(self) -> &'static [(&'static str, u64)] {
        match self {
            Self::Claude => HOOK_EVENTS,
            Self::Codex => CODEX_EVENTS,
            Self::Gemini => GEMINI_EVENTS,
            Self::Copilot => COPILOT_EVENTS,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookStatus {
    pub installed: bool,
    pub settings_path: String,
    pub hook_path: String,
    pub hook_ready: bool,
    pub agent: String,
    pub cli_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookPreview {
    pub diff: String,
    pub backup: String,
    pub settings_path: String,
    pub fingerprint: String,
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "USERPROFILE is unavailable; no configuration was changed.".into())
}

fn configuration_path(agent: Agent) -> Result<PathBuf, String> {
    let path = match agent {
        Agent::Claude => std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| home().map(|p| p.join(".claude")))?
            .join("settings.json"),
        Agent::Codex => std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| home().map(|p| p.join(".codex")))?
            .join("hooks.json"),
        // GEMINI_CLI_HOME replaces the system home, not the .gemini directory.
        Agent::Gemini => std::env::var_os("GEMINI_CLI_HOME")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(home)?
            .join(".gemini")
            .join("settings.json"),
        Agent::Copilot => std::env::var_os("COPILOT_HOME")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| home().map(|p| p.join(".copilot")))?
            .join("hooks")
            .join("coucou.json"),
    };
    if !path.is_absolute() {
        return Err(format!(
            "Configuration path must be absolute: {}",
            path.display()
        ));
    }
    Ok(path)
}

pub fn settings_path() -> PathBuf {
    configuration_path(Agent::Claude).unwrap_or_else(|_| PathBuf::from(".claude/settings.json"))
}

struct Snapshot {
    value: Value,
    bytes: Option<Vec<u8>>,
}

fn parse_settings(bytes: &[u8], path: &Path) -> Result<Value, String> {
    let text = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    // An existing blank file is invalid JSON. Treating it as absent could erase
    // a file that another editor has just truncated while saving.
    let value: Value = serde_json::from_slice(text).map_err(|error| {
        format!(
            "{} isn't valid JSON ({error}). Coucou won't overwrite it.",
            path.display()
        )
    })?;
    if !value.is_object() {
        return Err(format!(
            "{} isn't a JSON object. Coucou won't touch it.",
            path.display()
        ));
    }
    Ok(value)
}

fn validate(value: &Value, agent: Agent) -> Result<(), String> {
    if agent == Agent::Copilot
        && (value.get("version").is_some() || value.get("hooks").is_some())
        && value["version"].as_u64() != Some(1)
    {
        return Err("Copilot hook files require version: 1; nothing was changed.".into());
    }
    if let Some(hooks) = value.get("hooks") {
        let hooks = hooks
            .as_object()
            .ok_or("The hooks field must be an object; nothing was changed.")?;
        for (event, groups) in hooks {
            let groups = groups
                .as_array()
                .ok_or_else(|| format!("hooks.{event} must be an array; nothing was changed."))?;
            for group in groups {
                if !group.is_object() {
                    return Err(format!("hooks.{event} contains an invalid handler."));
                }
                if agent == Agent::Copilot {
                    if group.get("hooks").is_some() {
                        return Err(format!("hooks.{event} requires flat Copilot handlers."));
                    }
                } else {
                    let handlers =
                        group
                            .get("hooks")
                            .and_then(Value::as_array)
                            .ok_or_else(|| {
                                format!("hooks.{event} contains an invalid matcher group.")
                            })?;
                    if !handlers.iter().all(Value::is_object) {
                        return Err(format!("hooks.{event} contains an invalid handler."));
                    }
                }
            }
        }
    }
    Ok(())
}

fn read_snapshot(path: &Path, agent: Agent) -> Result<Snapshot, String> {
    let bytes = match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(format!(
                    "{} must be a regular file, not a link.",
                    path.display()
                ));
            }
            if meta.len() > MAX_CONFIG_BYTES {
                return Err("Hook configuration exceeds 4 MiB.".into());
            }
            let mut bytes = Vec::new();
            File::open(path)
                .map_err(|e| format!("Can't read {}: {e}", path.display()))?
                .take(MAX_CONFIG_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_CONFIG_BYTES {
                return Err("Hook configuration exceeds 4 MiB.".into());
            }
            Some(bytes)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("Can't read {}: {e}", path.display())),
    };
    let value = match &bytes {
        Some(bytes) => parse_settings(bytes, path)?,
        None => json!({}),
    };
    validate(&value, agent)?;
    Ok(Snapshot { value, bytes })
}

fn safe_executable(binary: &Path) -> Result<String, String> {
    let path = binary.to_str().ok_or("Relay path is not UTF-8.")?;
    if !binary.is_absolute()
        || path
            .chars()
            .any(|c| matches!(c, '\r' | '\n' | '"' | '%' | '!' | '^' | '$' | '`'))
    {
        return Err("Relay path contains characters unsafe for the provider's shell.".into());
    }
    Ok(path.strip_prefix(r"\\?\").unwrap_or(path).to_owned())
}

fn command(binary: &Path, agent: Agent, event: &str) -> Result<String, String> {
    let path = safe_executable(binary)?;
    let suffix = format!(" --agent {} --event {event} {MARKER}", agent.name());
    if agent == Agent::Codex {
        let bytes = path.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
            && bytes.iter().enumerate().all(|(i, b)| {
                b.is_ascii_alphanumeric()
                    || matches!(b, b'_' | b'.' | b'-' | b'\\')
                    || (i == 1 && *b == b':')
            })
        {
            // Valid as an executable token in both PowerShell and cmd.exe.
            return Ok(format!("{path}{suffix}"));
        }
        // A quoted binary alone is a string in PowerShell; an explicit launcher
        // also works when Codex falls back to cmd.exe.
        return Ok(format!(
            "{CODEX_LAUNCHER}{}'{suffix}\"",
            path.replace('\'', "''")
        ));
    }
    Ok(format!("\"{}\"{suffix}", path.replace('\\', "/")))
}

fn relay_filename(path: &str) -> bool {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("coucou-hook.exe"))
}

fn owned(handler: &Value, agent: Agent, event: &str) -> bool {
    if handler.get("type").and_then(Value::as_str) != Some("command") {
        return false;
    }
    if agent == Agent::Copilot {
        return handler
            .get("exec")
            .and_then(Value::as_str)
            .is_some_and(relay_filename)
            && handler.get("args")
                == Some(&json!(["--agent", "copilot", "--event", event, MARKER]))
            && ["bash", "powershell", "command"]
                .iter()
                .all(|key| handler.get(*key).is_none());
    }
    let Some(command) = handler.get("command").and_then(Value::as_str) else {
        return false;
    };
    let suffix = format!(" --agent {} --event {event} {MARKER}", agent.name());
    if let Some(prefix) = command.strip_suffix(&suffix) {
        let path = prefix
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(prefix);
        return relay_filename(path);
    }
    // Codex launcher keeps the suffix within the -Command string.
    if let Some(rest) = command.strip_prefix(CODEX_LAUNCHER) {
        if let Some(path) = rest.strip_suffix(&format!("'{suffix}\"")) {
            return relay_filename(&path.replace("''", "'"));
        }
    }
    // Only Claude had unmarked legacy hooks. Require the exact quoted relay
    // filename and supported event, not a substring in an unrelated command.
    agent == Agent::Claude
        && HOOK_EVENTS.iter().any(|(known, _)| *known == event)
        && command
            .strip_suffix(&format!(" {event}"))
            .and_then(|s| s.strip_prefix('"'))
            .and_then(|s| s.strip_suffix('"'))
            .is_some_and(relay_filename)
}

fn handlers<'a>(root: &'a Value, agent: Agent, event: &str) -> Vec<&'a Value> {
    let Some(groups) = root["hooks"][event].as_array() else {
        return vec![];
    };
    if agent == Agent::Copilot {
        groups.iter().collect()
    } else {
        groups
            .iter()
            .filter_map(|g| g["hooks"].as_array())
            .flatten()
            .collect()
    }
}

fn fragment(agent: Agent, binary: &Path) -> Result<Map<String, Value>, String> {
    let mut hooks = Map::new();
    for (event, timeout) in agent.events() {
        let mut handler = if agent == Agent::Copilot {
            json!({"type":"command", "exec":safe_executable(binary)?, "args":["--agent","copilot","--event",event,MARKER], "timeoutSec":timeout})
        } else {
            json!({"type":"command", "command":command(binary, agent, event)?, "timeout":timeout})
        };
        if agent == Agent::Gemini {
            handler["name"] = Value::String(format!("coucou-{event}"));
        }
        hooks.insert(
            (*event).into(),
            if agent == Agent::Copilot {
                json!([handler])
            } else {
                json!([{"hooks":[handler]}])
            },
        );
    }
    Ok(hooks)
}

fn configured_as_desired(root: &Value, source: &Map<String, Value>, agent: Agent) -> bool {
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    let count = hooks
        .keys()
        .flat_map(|event| {
            handlers(root, agent, event)
                .into_iter()
                .map(move |h| (event, h))
        })
        .filter(|(event, h)| owned(h, agent, event))
        .count();
    count == source.len()
        && source.iter().all(|(event, expected)| {
            if agent == Agent::Copilot {
                let own: Vec<Value> = handlers(root, agent, event)
                    .into_iter()
                    .filter(|h| owned(h, agent, event))
                    .cloned()
                    .collect();
                return Value::Array(own) == *expected;
            }
            let wanted = &expected[0];
            hooks[event].as_array().is_some_and(|groups| {
                groups
                    .iter()
                    .filter(|group| {
                        let mut own = (*group).clone();
                        own["hooks"]
                            .as_array_mut()
                            .expect("validated group")
                            .retain(|h| owned(h, agent, event));
                        own == *wanted
                    })
                    .count()
                    == 1
            })
        })
}

fn prune(root: &mut Value, agent: Agent) {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    hooks.retain(|event, value| {
        let groups = value.as_array_mut().expect("validated hooks array");
        let before = groups.len();
        if agent == Agent::Copilot {
            groups.retain(|handler| !owned(handler, agent, event));
        } else {
            groups.retain_mut(|group| {
                let handlers = group
                    .get_mut("hooks")
                    .and_then(Value::as_array_mut)
                    .expect("validated group");
                let count = handlers.len();
                handlers.retain(|handler| !owned(handler, agent, event));
                // Empty third-party groups already in the file remain untouched.
                count == 0 || !handlers.is_empty()
            });
        }
        before == 0 || !groups.is_empty()
    });
    if hooks.is_empty() {
        root.as_object_mut()
            .expect("validated root")
            .remove("hooks");
    }
}

fn desired(existing: &Value, agent: Agent, install: bool, binary: &Path) -> Result<Value, String> {
    validate(existing, agent)?;
    if !install {
        let mut next = existing.clone();
        prune(&mut next, agent);
        return Ok(next);
    }
    let source = fragment(agent, binary)?;
    if configured_as_desired(existing, &source, agent) {
        return Ok(existing.clone());
    }
    let mut next = existing.clone();
    prune(&mut next, agent);
    if agent == Agent::Copilot {
        next["version"] = json!(1);
    }
    let destination = next
        .as_object_mut()
        .expect("validated root")
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("validated hooks");
    for (event, groups) in source {
        destination
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("validated event")
            .extend(groups.as_array().expect("generated groups").iter().cloned());
    }
    Ok(next)
}

fn fingerprint(path: &Path, snapshot: &Snapshot, agent: Agent, install: bool) -> String {
    // This is a change token, not an authentication mechanism. Include agent,
    // action, path and existence so a preview cannot apply to a different file.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let prefix = format!(
        "{}\0{}\0{}\0{}\0",
        path.display(),
        agent.name(),
        install,
        snapshot.bytes.is_some()
    );
    for byte in prefix
        .as_bytes()
        .iter()
        .chain(snapshot.bytes.as_deref().unwrap_or_default())
    {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

fn cli_available(agent: Agent) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let name = if agent == Agent::Claude {
        "claude"
    } else {
        agent.name()
    };
    std::env::split_paths(&paths).any(|dir| {
        [".exe", ".cmd", ".bat", ".ps1", ""]
            .iter()
            .any(|extension| dir.join(format!("{name}{extension}")).is_file())
    })
}

fn status_at(agent: Agent, path: &Path, binary: &Path) -> Result<HookStatus, String> {
    let current = read_snapshot(path, agent)?;
    // Also recognize legacy/partial installations so they can be removed or
    // repaired. Presence is configuration evidence, not CLI compatibility.
    let installed = current
        .value
        .get("hooks")
        .and_then(Value::as_object)
        .is_some_and(|hooks| {
            hooks.keys().any(|event| {
                handlers(&current.value, agent, event)
                    .iter()
                    .any(|handler| owned(handler, agent, event))
            })
        });
    Ok(HookStatus {
        installed,
        settings_path: path.display().to_string(),
        hook_path: binary.display().to_string(),
        hook_ready: binary.is_file(),
        agent: agent.name().into(),
        cli_available: cli_available(agent),
    })
}

pub fn agent_status(agent: &str) -> Result<HookStatus, String> {
    let agent = Agent::parse(agent)?;
    status_at(
        agent,
        &configuration_path(agent)?,
        &settings::hook_exe_path(),
    )
}

pub fn status() -> HookStatus {
    agent_status("claude").unwrap_or_else(|_| HookStatus {
        installed: false,
        settings_path: settings_path().display().to_string(),
        hook_path: settings::hook_exe_path().display().to_string(),
        hook_ready: settings::hook_exe_path().is_file(),
        agent: "claude".into(),
        cli_available: cli_available(Agent::Claude),
    })
}

fn adjacent(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .expect("configuration file name")
        .to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

fn backup_candidate(path: &Path, token: &str) -> PathBuf {
    let time = unsafe { GetLocalTime() };
    let date = format!("{:04}{:02}{:02}", time.wYear, time.wMonth, time.wDay);
    for counter in 0..10000 {
        let path = adjacent(path, &format!(".coucou.bak-{date}-{token}-{counter}"));
        if !path.exists() {
            return path;
        }
    }
    adjacent(path, &format!(".coucou.bak-{date}-{}", unique_suffix()))
}

fn preview_at(
    agent: Agent,
    path: &Path,
    binary: &Path,
    install: bool,
) -> Result<HookPreview, String> {
    let current = read_snapshot(path, agent)?;
    let next = desired(&current.value, agent, install, binary)?;
    let token = fingerprint(path, &current, agent, install);
    Ok(HookPreview {
        diff: super::unified_diff(&pretty(&current.value), &pretty(&next)),
        backup: if current.bytes.is_some() && current.value != next {
            backup_candidate(path, &token).display().to_string()
        } else {
            String::new()
        },
        settings_path: path.display().to_string(),
        fingerprint: token,
    })
}

pub fn agent_preview(agent: &str, install: bool) -> Result<HookPreview, String> {
    let agent = Agent::parse(agent)?;
    preview_at(
        agent,
        &configuration_path(agent)?,
        &settings::hook_exe_path(),
        install,
    )
}
pub fn preview(install: bool) -> Result<HookPreview, String> {
    agent_preview("claude", install)
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON value serialization")
}

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id()
    )
}

fn safe_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("Configuration has no parent directory.")?;
    for component in parent.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if component.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(component) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => (),
            Ok(_) => {
                return Err(format!(
                    "Configuration directory is a link or not a directory: {}",
                    component.display()
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(component).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

fn lock_configuration(path: &Path) -> Result<File, String> {
    let path = adjacent(path, ".coucou.lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("Unsafe hook configuration lock file.".into());
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Kernel-enforced exclusive sharing, released even if Coucou crashes.
        // Keep the lock file to avoid a delete/recreate inode race.
        options.share_mode(0);
    }
    options
        .open(&path)
        .map_err(|e| format!("Hook configuration is busy or its lock cannot be opened: {e}"))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error.to_string());
    }
    Ok(())
}

fn write_at(
    agent: Agent,
    path: &Path,
    binary: &Path,
    install: bool,
    token: &str,
) -> Result<String, String> {
    safe_parent(path)?;
    let _lock = lock_configuration(path)?;
    let current = read_snapshot(path, agent)?;
    if fingerprint(path, &current, agent, install) != token {
        return Err(format!(
            "{} changed since the preview. Nothing was written; review a new diff.",
            path.display()
        ));
    }
    if install && !binary.is_file() {
        return Err(
            "coucou-hook.exe is unavailable. Restart Coucou before installing hooks.".into(),
        );
    }
    let next = desired(&current.value, agent, install, binary)?;
    // No rewrite, backup or whitespace changes on reinstall/uninstall no-ops.
    if next == current.value {
        return Ok(String::new());
    }
    let backup = if let Some(bytes) = &current.bytes {
        let backup = backup_candidate(path, token);
        write_new(&backup, bytes).map_err(|e| format!("Backup failed: {e}"))?;
        Some(backup)
    } else {
        None
    };
    let temp = adjacent(path, &format!(".coucou-{}.tmp", unique_suffix()));
    let bytes = format!("{}\n", pretty(&next)).into_bytes();
    write_new(&temp, &bytes).map_err(|e| format!("Writing temporary configuration failed: {e}"))?;
    if let Ok(meta) = fs::metadata(path) {
        fs::set_permissions(&temp, meta.permissions()).map_err(|e| e.to_string())?;
    }
    let result = (|| {
        let live = read_snapshot(path, agent)?;
        if live.bytes != current.bytes {
            return Err("Configuration changed before replacement; review a new diff.".into());
        }
        // Rust uses an atomic replace-existing rename on Windows, on the same volume.
        fs::rename(&temp, path).map_err(|e| format!("Replacing configuration failed: {e}"))?;
        match read_snapshot(path, agent) {
            Ok(actual) if actual.bytes.as_deref() == Some(bytes.as_slice()) => Ok(()),
            Ok(_) => Err("Configuration changed immediately after replacement; preserved the concurrent edit.".into()),
            Err(error) => {
                // A failed verification cannot leave an unusable configuration.
                // Restore only if the current bytes are still ours.
                if fs::read(path).ok().as_deref() == Some(bytes.as_slice()) {
                    let restored = if let Some(original) = &current.bytes {
                        let rollback = adjacent(path, &format!(".coucou-{}.rollback", unique_suffix()));
                        write_new(&rollback, original).and_then(|_| fs::rename(&rollback, path).map_err(|e| e.to_string()))
                    } else { fs::remove_file(path).map_err(|e| e.to_string()) };
                    if let Err(rollback) = restored { return Err(format!("{error}; rollback failed: {rollback}. Backup: {}", backup.as_ref().map(|p| p.display().to_string()).unwrap_or_default())); }
                }
                Err(error)
            }
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(backup.map(|p| p.display().to_string()).unwrap_or_default())
}

pub fn agent_write(agent: &str, install: bool, fingerprint: &str) -> Result<String, String> {
    let agent = Agent::parse(agent)?;
    write_at(
        agent,
        &configuration_path(agent)?,
        &settings::hook_exe_path(),
        install,
        fingerprint,
    )
}
pub fn write(install: bool, fingerprint: &str) -> Result<String, String> {
    agent_write("claude", install, fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary() -> PathBuf {
        PathBuf::from(r"C:\Program Files\Coucou\coucou-hook.exe")
    }

    #[test]
    fn malformed_existing_files_fail_closed() {
        for bad in [b"".as_slice(), b"  ", b"{ broken", b"[1,2]"] {
            assert!(parse_settings(bad, Path::new("settings.json")).is_err());
        }
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(br#"{"model":"existing"}"#);
        assert_eq!(
            parse_settings(&bytes, Path::new("settings.json")).unwrap()["model"],
            "existing"
        );
        for config in [
            json!({"hooks":[]}),
            json!({"hooks":{"PreToolUse":{}}}),
            json!({"hooks":{"PreToolUse":[{"hooks":[null]}]}}),
        ] {
            assert!(desired(&config, Agent::Claude, true, &binary()).is_err());
        }
        assert!(desired(
            &json!({"version":2,"hooks":{}}),
            Agent::Copilot,
            true,
            &binary()
        )
        .is_err());
    }

    #[test]
    fn all_formats_have_correct_units_and_no_new_approval_handlers() {
        for agent in [Agent::Claude, Agent::Codex, Agent::Gemini, Agent::Copilot] {
            let config = desired(&json!({}), agent, true, &binary()).unwrap();
            assert!(configured_as_desired(
                &config,
                &fragment(agent, &binary()).unwrap(),
                agent
            ));
            if agent != Agent::Claude {
                assert!(config["hooks"]["PermissionRequest"].is_null());
                assert!(config["hooks"]["permissionRequest"].is_null());
            }
            if agent == Agent::Gemini {
                assert_eq!(
                    config["hooks"]["BeforeTool"][0]["hooks"][0]["timeout"],
                    10000
                );
            }
            if agent == Agent::Codex {
                assert_eq!(config["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"], 3);
            }
            if agent == Agent::Copilot {
                assert_eq!(config["version"], 1);
                assert!(config["hooks"]["preToolUse"][0]["exec"].is_string());
            }
        }
    }

    #[test]
    fn mixed_groups_preserve_every_foreign_handler_and_metadata() {
        for agent in [Agent::Claude, Agent::Codex, Agent::Gemini] {
            let event = agent.events()[0].0;
            let own = fragment(agent, &binary()).unwrap()[event][0]["hooks"][0].clone();
            let foreign = json!({"type":"command","command":"other-tool coucou-hook --observe"});
            let original = json!({"model":"existing","hooks":{event:[{"matcher":"special","custom":true,"hooks":[foreign.clone(),own]}],"Unknown":[{"hooks":[]}]}});
            let cleaned = desired(&original, agent, false, &binary()).unwrap();
            assert_eq!(cleaned["hooks"][event][0]["hooks"], json!([foreign]));
            assert_eq!(cleaned["hooks"][event][0]["matcher"], "special");
            assert_eq!(cleaned["hooks"][event][0]["custom"], true);
            assert_eq!(cleaned["hooks"]["Unknown"], original["hooks"]["Unknown"]);
        }
    }

    #[test]
    fn legacy_claude_is_migrated_without_a_substring_ownership_bug() {
        let original = json!({"hooks":{"PreToolUse":[{"hooks":[
            {"type":"command","command":"\"C:/old/coucou-hook.exe\" PreToolUse"},
            {"type":"command","command":"echo coucou-hook third-party"}
        ]}]}});
        let installed = desired(&original, Agent::Claude, true, &binary()).unwrap();
        assert_eq!(handlers(&installed, Agent::Claude, "PreToolUse").len(), 2);
        let removed = desired(&installed, Agent::Claude, false, &binary()).unwrap();
        assert_eq!(
            removed["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "echo coucou-hook third-party"
        );
    }

    #[test]
    fn reinstall_is_idempotent_and_foreign_order_is_preserved() {
        for agent in [Agent::Claude, Agent::Codex, Agent::Gemini, Agent::Copilot] {
            let event = agent.events()[0].0;
            let foreign = if agent == Agent::Copilot {
                json!({"type":"command","exec":"foreign.exe","args":[]})
            } else {
                json!({"hooks":[{"type":"command","command":"foreign.exe"}]})
            };
            let mut original = json!({"theme":"dark","hooks":{event:[foreign.clone()]}});
            if agent == Agent::Copilot {
                original["version"] = json!(1);
            }
            let installed = desired(&original, agent, true, &binary()).unwrap();
            assert_eq!(installed["hooks"][event][0], foreign);
            assert_eq!(
                desired(&installed, agent, true, &binary()).unwrap(),
                installed
            );
            assert_eq!(
                desired(&installed, agent, false, &binary()).unwrap(),
                original
            );
        }
    }

    #[test]
    fn filesystem_backup_stale_preview_and_byte_idempotence() {
        let root = std::env::temp_dir().join(format!("coucou-hook-test-{}", unique_suffix()));
        fs::create_dir_all(&root).unwrap();
        let binary = root.join("coucou-hook.exe");
        fs::write(&binary, b"test").unwrap();
        for agent in [Agent::Claude, Agent::Codex, Agent::Gemini, Agent::Copilot] {
            let path = root.join(format!("{}.json", agent.name()));
            let original = if agent == Agent::Copilot {
                br#"{"version":1,"theme":"dark"}"#.as_slice()
            } else {
                br#"{"model":"keep","theme":"dark"}"#.as_slice()
            };
            fs::write(&path, original).unwrap();
            let plan = preview_at(agent, &path, &binary, true).unwrap();
            let backup = write_at(agent, &path, &binary, true, &plan.fingerprint).unwrap();
            assert_eq!(fs::read(backup).unwrap(), original);
            let bytes = fs::read(&path).unwrap();
            let repeat = preview_at(agent, &path, &binary, true).unwrap();
            assert_eq!(repeat.diff, "No change.");
            assert!(write_at(agent, &path, &binary, true, &repeat.fingerprint)
                .unwrap()
                .is_empty());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            let stale = preview_at(agent, &path, &binary, false).unwrap();
            fs::write(&path, br#"{"theme":"edited"}"#).unwrap();
            assert!(write_at(agent, &path, &binary, false, &stale.fingerprint).is_err());
            assert_eq!(fs::read(&path).unwrap(), br#"{"theme":"edited"}"#);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn previews_bind_existence_path_agent_and_action() {
        let snapshot = Snapshot {
            value: json!({}),
            bytes: None,
        };
        let path = Path::new(r"C:\config.json");
        let token = fingerprint(path, &snapshot, Agent::Claude, true);
        assert_ne!(token, fingerprint(path, &snapshot, Agent::Claude, false));
        assert_ne!(token, fingerprint(path, &snapshot, Agent::Codex, true));
        assert_ne!(
            token,
            fingerprint(Path::new(r"C:\other.json"), &snapshot, Agent::Claude, true)
        );
        assert_ne!(
            token,
            fingerprint(
                path,
                &Snapshot {
                    value: json!({}),
                    bytes: Some(vec![])
                },
                Agent::Claude,
                true
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn concurrent_installations_cannot_acquire_the_same_lock() {
        let root = std::env::temp_dir().join(format!("coucou-lock-test-{}", unique_suffix()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let lock = lock_configuration(&path).unwrap();
        assert!(lock_configuration(&path).is_err());
        drop(lock);
        assert!(lock_configuration(&path).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}
