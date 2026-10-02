//! The same installed executable hosts a parser before initializing Tauri.
//! Job limits contain resource exhaustion; this is not an OS permission sandbox.
use super::*;
use std::{
    io::{Read, Write},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};

const WORKER_FLAG: &str = "--coucou-document-worker";
const MAX_REQUEST_BYTES: u64 = 4096;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const WORKER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize)]
struct Request {
    attachment: Attachment,
}
#[derive(Serialize, Deserialize)]
struct Response {
    extraction: Option<Extraction>,
    error: Option<String>,
}

/// Call from main before Tauri: if let Some(code) = worker_entry() { exit(code) }.
/// There is no user prompt, network client, shell, macro engine or office runtime.
pub fn worker_entry() -> Option<i32> {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new(WORKER_FLAG)) {
        return None;
    }
    if std::env::args_os().count() != 2 {
        return Some(2);
    }
    let result = (|| {
        let mut request = Vec::new();
        std::io::stdin()
            .take(MAX_REQUEST_BYTES + 1)
            .read_to_end(&mut request)
            .map_err(|_| "Pedido de extração inválido.")?;
        if request.len() as u64 > MAX_REQUEST_BYTES {
            return Err("Pedido de extração excede o limite seguro.".to_string());
        }
        let request: Request =
            serde_json::from_slice(&request).map_err(|_| "Pedido de extração inválido.")?;
        storage::validate_id(&request.attachment.id)?;
        if request.attachment.size == 0 || request.attachment.size > MAX_FILE_BYTES {
            return Err("O anexo excede o limite seguro.".into());
        }
        let cwd = std::env::current_dir().map_err(|_| "A cópia do anexo não está disponível.")?;
        storage::reject_links(&cwd.join("original.bin"))?;
        let extraction = extract::extract_file(&cwd.join("original.bin"), &request.attachment)?;
        validate_content(&request.attachment, &extraction)?;
        Ok(extraction)
    })();
    let response = match result {
        Ok(extraction) => Response {
            extraction: Some(extraction),
            error: None,
        },
        Err(error) => Response {
            extraction: None,
            error: Some(error),
        },
    };
    match serde_json::to_vec(&response) {
        Ok(bytes) if bytes.len() <= MAX_RESPONSE_BYTES => {
            if std::io::stdout().write_all(&bytes).is_ok() {
                Some(0)
            } else {
                Some(2)
            }
        }
        _ => Some(2),
    }
}

struct Job(HANDLE);
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
            let job = Self(
                CreateJobObjectW(None, None)
                    .map_err(|_| "Não foi possível conter o extrator de documentos.")?,
            );
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_PROCESS_TIME;
            limits.BasicLimitInformation.ActiveProcessLimit = 1;
            limits.BasicLimitInformation.PerProcessUserTimeLimit = 30 * 10_000_000;
            limits.ProcessMemoryLimit = 512 * 1024 * 1024;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
            .map_err(|_| "Não foi possível limitar os recursos do extrator.")?;
            Ok(job)
        }
    }
    fn attach(&self, pid: u32) -> Result<(), String> {
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
                .map_err(|_| "Não foi possível associar o extrator à contenção.")?;
            let result = AssignProcessToJobObject(self.0, process);
            let _ = CloseHandle(process);
            result.map_err(|_| "Não foi possível conter o extrator.")?;
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
                .map_err(|_| "Não foi possível iniciar o extrator contido.")?;
            let mut entry = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            let mut next = Thread32First(snapshot, &mut entry).is_ok();
            let mut resumed = false;
            while next {
                if entry.th32OwnerProcessID == pid {
                    if let Ok(thread) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
                    {
                        resumed = ResumeThread(thread) != u32::MAX;
                        let _ = CloseHandle(thread);
                    }
                    break;
                }
                next = Thread32Next(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
            if resumed {
                Ok(())
            } else {
                Err("Não foi possível iniciar o extrator contido.".into())
            }
        }
    }
    fn terminate(&self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

pub(super) async fn extract(
    directory: &std::path::Path,
    attachment: &Attachment,
    cancel: watch::Receiver<bool>,
) -> Result<Extraction, String> {
    let executable =
        std::env::current_exe().map_err(|_| "Não foi possível localizar o extrator do Coucou.")?;
    run_program(
        directory,
        attachment,
        cancel,
        &executable,
        &[WORKER_FLAG.into()],
        WORKER_TIMEOUT,
    )
    .await
}

async fn run_program(
    directory: &std::path::Path,
    attachment: &Attachment,
    cancel: watch::Receiver<bool>,
    executable: &std::path::Path,
    arguments: &[String],
    deadline: Duration,
) -> Result<Extraction, String> {
    if *cancel.borrow() {
        return Err("Preparação cancelada.".into());
    }
    storage::reject_links(directory)?;
    let job = Job::new()?;
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(directory)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x08000000 | CREATE_SUSPENDED.0);
    if let Some(system_root) = std::env::var_os("SYSTEMROOT") {
        command.env("SYSTEMROOT", system_root);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Não foi possível iniciar a preparação do anexo.")?;
    if let Err(error) = child
        .id()
        .ok_or_else(|| "O extrator não recebeu um identificador.".to_string())
        .and_then(|pid| job.attach(pid))
    {
        job.terminate();
        let _ = child.start_kill();
        return Err(error);
    }
    let request = serde_json::to_vec(&Request {
        attachment: attachment.clone(),
    })
    .map_err(|_| "Pedido de preparação inválido.")?;
    if request.len() as u64 > MAX_REQUEST_BYTES {
        job.terminate();
        return Err("Pedido de preparação excede o limite seguro.".into());
    }
    let mut stdin = child
        .stdin
        .take()
        .ok_or("O extrator não disponibilizou sua entrada.")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("O extrator não disponibilizou sua saída.")?;
    let result = tokio::select! { biased;
        _ = cancelled(cancel.clone()) => Err("Preparação cancelada.".to_string()),
        result = tokio::time::timeout(deadline, async {
            stdin.write_all(&request).await.map_err(|_| "Não foi possível enviar o anexo ao extrator.")?;
            stdin.shutdown().await.map_err(|_| "Não foi possível concluir o pedido de extração.")?;
            drop(stdin);
            let mut bytes = Vec::new();
            stdout.take((MAX_RESPONSE_BYTES + 1) as u64).read_to_end(&mut bytes).await.map_err(|_| "Não foi possível ler a preparação do anexo.")?;
            if bytes.len() > MAX_RESPONSE_BYTES { return Err("A extração excedeu o limite seguro de saída.".into()); }
            let status = child.wait().await.map_err(|_| "Não foi possível confirmar o término do extrator.")?;
            if !status.success() { return Err("O extrator foi encerrado por arquivo inválido ou limite de recursos.".into()); }
            let response: Response = serde_json::from_slice(&bytes).map_err(|_| "O extrator retornou um resultado inválido.")?;
            match (response.extraction, response.error) {
                (Some(extraction), None) => {
                    if extraction.text.chars().count() > MAX_EXTRACTED_CHARS || extraction.references.len() > 30_000 || extraction.references.iter().any(|reference| reference.attachment_id != attachment.id) { return Err("A extração excedeu seus limites de conteúdo.".into()); }
                    validate_content(attachment, &extraction)?;
                    Ok(extraction)
                }
                (None, Some(error)) if error.len() < 1000 && !error.contains('\0') => Err(crate::privacy::diagnostic(&error)),
                _ => Err("O extrator retornou um estado inválido.".into()),
            }
        }) => result.unwrap_or_else(|_| Err("A extração excedeu 30 segundos e foi encerrada. Divida o documento e tente novamente.".into())),
    };
    job.terminate();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    result
}

#[cfg(test)]
pub(super) async fn fixture(
    directory: &std::path::Path,
    attachment: &Attachment,
    cancel: watch::Receiver<bool>,
    script: &str,
    deadline: Duration,
) -> Result<Extraction, String> {
    let node = std::env::var_os("PATH")
        .expect("Node is required by the existing frontend tests")
        .to_string_lossy()
        .split(';')
        .map(|directory| std::path::Path::new(directory.trim_matches('"')).join("node.exe"))
        .find(|path| path.is_file())
        .expect("node.exe is required for document worker containment tests");
    run_program(
        directory,
        attachment,
        cancel,
        &node,
        &["--eval".into(), script.into()],
        deadline,
    )
    .await
}
