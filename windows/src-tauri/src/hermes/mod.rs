//! Managed Hermes ACP integration. The model can only reach the Coucou broker.
//!
//! ACP is a protocol, not a sandbox: the owned bootstrap checks the effective
//! tool surface and refuses a changed runtime before any prompt is submitted.
mod acp;
mod profile;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub use acp::run;
pub use profile::{prepare_launch, runtime_status, seed_memory};

pub const HERMES_REVISION: &str = "781334eea4b9225a3e194faf0c241d9afe218634";
pub const MIN_CONTEXT_LENGTH: u32 = 64_000;
pub const BRIDGE_TOOLS: [&str; 16] = [
    "context_read",
    "files_request",
    "files_count",
    "files_list",
    "document_read",
    "memory_propose",
    "skill_propose",
    "delegate_cli",
    "open_device_manager",
    "open_windows_settings",
    "open_application",
    "open_directory",
    "query_processes",
    "organize_files_dry_run",
    "query_recycle_bin",
    "cli_version",
];
pub const EFFECTIVE_TOOL_NAMES: [&str; 16] = [
    "mcp__coucou__context_read",
    "mcp__coucou__files_request",
    "mcp__coucou__files_count",
    "mcp__coucou__files_list",
    "mcp__coucou__document_read",
    "mcp__coucou__memory_propose",
    "mcp__coucou__skill_propose",
    "mcp__coucou__delegate_cli",
    "mcp__coucou__open_device_manager",
    "mcp__coucou__open_windows_settings",
    "mcp__coucou__open_application",
    "mcp__coucou__open_directory",
    "mcp__coucou__query_processes",
    "mcp__coucou__organize_files_dry_run",
    "mcp__coucou__query_recycle_bin",
    "mcp__coucou__cli_version",
];

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeConfig {
    pub python_executable: String,
    pub source_directory: String,
}

/// Credentials are passed through an environment cleared by the process owner.
/// Deliberately lacks Debug/Serialize to prevent accidental credential logging.
pub struct ModelConnection {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub context_length: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeSpec {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

pub struct LaunchSpec {
    pub program: PathBuf,
    pub arguments: Vec<String>,
    /// Caller MUST use env_clear() before applying this map.
    pub environment: BTreeMap<String, String>,
    pub cwd: PathBuf,
    pub profile_home: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub available: bool,
    pub revision: &'static str,
    pub python_executable: Option<String>,
    pub source_directory: Option<String>,
    pub detail: String,
    /// A source check alone does not qualify the live ACP tool surface.
    pub live_isolation_qualified: bool,
}
