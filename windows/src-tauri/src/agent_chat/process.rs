//! Resolve an installed executable without evaluating shell commands or npm shims.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command as AsyncCommand};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};

const NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Debug)]
pub struct Executable {
    pub program: PathBuf,
    pub prefix: Vec<String>,
    pub display: PathBuf,
}

/// Canonicalization adds a verbatim prefix on Windows. Node launch scripts and
/// agent protocol paths need ordinary absolute drive/UNC paths instead.
pub(super) fn cli_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        return PathBuf::from(format!("\\\\{unc}"));
    }
    if let Some(drive) = text.strip_prefix("\\\\?\\") {
        if drive.as_bytes().get(1) == Some(&b':') {
            return PathBuf::from(drive);
        }
    }
    path.to_path_buf()
}

fn find(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(';')
        .find_map(|directory| {
            let directory = Path::new(directory.trim_matches('"'));
            [".exe", ".cmd", ".ps1"]
                .iter()
                .map(|ext| directory.join(format!("{name}{ext}")))
                .find(|path| path.is_file())
        })
}

pub fn resolve(agent: &str) -> Result<Executable, String> {
    let name = match agent {
        "codex" => "codex",
        "claude" => "claude",
        "gemini" => "gemini",
        "copilot" => "copilot",
        _ => return Err("Unknown CLI agent.".into()),
    };
    let path = find(name).ok_or_else(|| {
        format!("{name} CLI is not on PATH. Install and sign in to it, then restart Coucou.")
    })?;
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Ok(Executable {
            program: path.clone(),
            prefix: vec![],
            display: path,
        });
    }
    let package = match agent {
        "codex" => "@openai/codex",
        "claude" => "@anthropic-ai/claude-code",
        "gemini" => "@google/gemini-cli",
        _ => "@github/copilot",
    };
    let directory = path
        .parent()
        .ok_or("Invalid CLI location.")?
        .join("node_modules")
        .join(package);
    let package_json:Value=serde_json::from_slice(&std::fs::read(directory.join("package.json")).map_err(|_|format!("{name} has an unsupported launcher. Install its official executable or npm package."))?).map_err(|_|"Invalid installed CLI package.")?;
    let bin = package_json
        .get("bin")
        .and_then(|bin| bin.as_str().or_else(|| bin.get(name)?.as_str()))
        .ok_or("Installed CLI package has no documented executable.")?;
    let script = directory
        .join(bin)
        .canonicalize()
        .map_err(|_| "Installed CLI executable is missing.")?;
    let canonical_dir = directory
        .canonicalize()
        .map_err(|_| "Installed CLI package is missing.")?;
    if !script.starts_with(canonical_dir) {
        return Err("CLI package executable escapes its installation directory.".into());
    }
    let node = find("node")
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        })
        .ok_or("This CLI needs node.exe on PATH.")?;
    Ok(Executable {
        program: node,
        prefix: vec![cli_path(&script).to_string_lossy().into_owned()],
        display: path,
    })
}

pub async fn gemini_flag(
    executable: &Executable,
    cwd: &Path,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<String, String> {
    if *cancel.borrow() {
        return Err("Turn cancelled.".into());
    }
    let (mut child, job) = spawn(executable, &["--help".into()], cwd)?;
    child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or("Gemini help output unavailable.")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Gemini help error output unavailable.")?;
    let result = tokio::select! {biased;
        _=cancel.changed()=>Err("Turn cancelled.".to_string()),
        result=tokio::time::timeout(Duration::from_secs(10),async{
            let mut bytes=Vec::new();
            let mut stdout=stdout.take(65537);
            let mut stderr=tokio::io::BufReader::new(stderr);
            let mut discard=tokio::io::sink();
            let read=stdout.read_to_end(&mut bytes);
            let drain=tokio::io::copy(&mut stderr,&mut discard);
            let (read,_,status)=tokio::join!(read,drain,child.wait());
            read.map_err(|_|"Unable to inspect Gemini CLI.".to_string())?;
            if bytes.len()>65536||!status.is_ok_and(|status|status.success()){return Err("Gemini help inspection failed.".to_string());}
            Ok(bytes)
        })=>result.map_err(|_|"Gemini help inspection timed out.".to_string()).and_then(|result|result),
    };
    job.terminate();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    let bytes = result?;
    let help = String::from_utf8_lossy(&bytes);
    if help.contains("--acp") {
        Ok("--acp".into())
    } else if help.contains("--experimental-acp") {
        Ok("--experimental-acp".into())
    } else {
        Err(
            "This Gemini CLI version does not expose ACP. Update the CLI before using its chat."
                .into(),
        )
    }
}

/// The process is created suspended, assigned to a non-inherited job, then resumed.
/// This avoids the race where an npm launcher spawns a child before job assignment.
pub struct Job(HANDLE);
// Job handles have no thread affinity; this handle is exclusively owned.
unsafe impl Send for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl Job {
    fn new() -> Result<Self, String> {
        unsafe {
            let handle =
                CreateJobObjectW(None, None).map_err(|_| "Cannot create a CLI process job.")?;
            let job = Self(handle);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
            .map_err(|_| "Cannot restrict the CLI process job.")?;
            Ok(job)
        }
    }
    fn attach_and_resume(&self, pid: u32) -> Result<(), String> {
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
                .map_err(|_| "Cannot open the suspended CLI.")?;
            let assigned = AssignProcessToJobObject(self.0, process);
            let _ = CloseHandle(process);
            assigned.map_err(|_| "Cannot contain the CLI process tree.")?;
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
                .map_err(|_| "Cannot find the suspended CLI thread.")?;
            let mut entry = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            let mut found = false;
            let mut next = Thread32First(snapshot, &mut entry).is_ok();
            while next {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                        .map_err(|_| "Cannot resume the CLI thread.");
                    if let Ok(thread) = thread {
                        found = ResumeThread(thread) != u32::MAX;
                        let _ = CloseHandle(thread);
                    }
                    break;
                }
                next = Thread32Next(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
            if found {
                Ok(())
            } else {
                Err("Cannot resume the contained CLI.".into())
            }
        }
    }
    pub fn terminate(&self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
    pub fn process_count(&self) -> Option<u32> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        unsafe {
            QueryInformationJobObject(
                Some(self.0),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
                None,
            )
            .ok()?;
        }
        Some(info.ActiveProcesses)
    }
}

pub fn spawn(executable: &Executable, args: &[String], cwd: &Path) -> Result<(Child, Job), String> {
    let job = Job::new()?;
    let mut command = AsyncCommand::new(&executable.program);
    command
        .args(&executable.prefix)
        .args(args)
        .current_dir(cli_path(cwd))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .creation_flags(NO_WINDOW | CREATE_SUSPENDED.0);
    let mut child = command
        .spawn()
        .map_err(|_| "Unable to start the installed CLI.")?;
    let pid = child.id().ok_or("CLI process has no ID.")?;
    if let Err(error) = job.attach_and_resume(pid) {
        job.terminate();
        let _ = child.start_kill();
        return Err(error);
    }
    Ok((child, job))
}
