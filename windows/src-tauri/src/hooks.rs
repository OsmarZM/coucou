// Hook installation for Claude Code, Codex CLI, Gemini CLI and Copilot CLI.
// Provider-specific JSON handling lives in installer; binary distribution and
// the compact settings diff remain shared here.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::settings;

mod installer;
pub use installer::{
    agent_preview, agent_status, agent_write, preview, status, write, HookPreview, HookStatus,
};

/// Copies coucou-hook.exe into %LOCALAPPDATA%\Coucou\bin on launch.
/// In a bundled install it comes from the app resources; in `tauri dev` it sits
/// next to coucou.exe in the workspace target directory.
///
/// Every candidate is tried rather than just the first, because getting this
/// wrong is silent and fatal: `resources` used to be a glob, which made NSIS
/// mirror the source path into `_up_\target\release\`, no candidate matched, and
/// the relay was simply never installed. It only looked healthy on a developer
/// machine, where a leftover copy from `tauri dev` was already sitting in bin/.
pub fn ensure_hook_exe(app: &AppHandle) {
    let dest = settings::hook_exe_path();
    let Some(dir) = dest.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = app
        .path()
        .resolve("coucou-hook.exe", tauri::path::BaseDirectory::Resource)
    {
        candidates.push(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            // Installed build, then `tauri dev` (target/debug) next to the
            // release hook the pre-build step produces.
            candidates.push(parent.join("coucou-hook.exe"));
            candidates.push(parent.join("../release/coucou-hook.exe"));
            // Belt and braces: where the old glob form used to land it.
            candidates.push(parent.join("_up_/target/release/coucou-hook.exe"));
        }
    }

    let tried: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
    let Some(src) = candidates.into_iter().find(|p| p.exists()) else {
        crate::log::line(format!(
            "coucou-hook.exe not found — Claude Code hooks cannot work. Looked in: {}",
            tried.join(", ")
        ));
        return;
    };

    let same = match (std::fs::metadata(&src), std::fs::metadata(&dest)) {
        (Ok(a), Ok(b)) => a.len() == b.len() && a.modified().ok() == b.modified().ok(),
        _ => false,
    };
    if same {
        return;
    }
    // A hook may be running right now and hold the file open; keeping the old
    // copy is fine, it is the same relay.
    if let Err(err) = std::fs::copy(&src, &dest) {
        if !dest.exists() {
            crate::log::line(format!("could not install coucou-hook.exe: {err}"));
        }
    }
}

// ── Minimal unified diff (LCS) ────────────────────────────────────────────────

/// settings.json is short, so a plain O(n·m) LCS is the simplest honest diff.
fn unified_diff(before: &str, after: &str) -> String {
    if before == after {
        return "No change.".into();
    }
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let (n, m) = (a.len(), b.len());

    // A legitimate large settings file must not allocate an unbounded n*m
    // matrix. Fall back to one exact changed block with shared edge context.
    if n.saturating_mul(m) > 1_000_000 {
        let prefix = a
            .iter()
            .zip(&b)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix = a[prefix..]
            .iter()
            .rev()
            .zip(b[prefix..].iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        let mut result = String::new();
        for line in &a[prefix.saturating_sub(3)..prefix] {
            result.push_str(&format!("  {line}\n"));
        }
        for line in &a[prefix..n - suffix] {
            result.push_str(&format!("- {line}\n"));
        }
        for line in &b[prefix..m - suffix] {
            result.push_str(&format!("+ {line}\n"));
        }
        for line in &a[n - suffix..(n - suffix + 3).min(n)] {
            result.push_str(&format!("  {line}\n"));
        }
        return result;
    }

    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut out: Vec<String> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(format!("  {}", a[i]));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(format!("- {}", a[i]));
            i += 1;
        } else {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }
    }
    while i < n {
        out.push(format!("- {}", a[i]));
        i += 1;
    }
    while j < m {
        out.push(format!("+ {}", b[j]));
        j += 1;
    }

    // Keep three lines of context around each change so the panel stays readable.
    let changed: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with('+') || l.starts_with('-'))
        .map(|(i, _)| i)
        .collect();
    if changed.is_empty() {
        return "No change.".into();
    }
    let mut keep = vec![false; out.len()];
    for idx in changed {
        let lo = idx.saturating_sub(3);
        let hi = (idx + 4).min(out.len());
        for line in keep.iter_mut().take(hi).skip(lo) {
            *line = true;
        }
    }
    let mut result = String::new();
    let mut gap = false;
    for (idx, line) in out.iter().enumerate() {
        if keep[idx] {
            result.push_str(line);
            result.push('\n');
            gap = false;
        } else if !gap {
            result.push_str("  …\n");
            gap = true;
        }
    }
    result
}
