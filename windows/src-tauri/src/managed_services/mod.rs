//! Optional, user-owned FreeLLMAPI infrastructure. Disabled unless the setup
//! decision and explicit provider/model configuration allow starting it.
mod config;
mod runtime;
mod vault;

use crate::agent_chat::process::Job;
pub use config::{ManagedServiceSettings, ModelSelection};
use runtime::ServiceIdentity;
use serde::Serialize;
use serde_json::json;
use std::{fs::File, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};

const PROVISION_SCRIPT: &str =
    include_str!("../../../scripts/managed-services/provision-free-models.ps1");
const AUTOSTART_SCRIPT: &str =
    include_str!("../../../scripts/managed-services/autostart-free-models.ps1");
const SERVER_SCRIPT: &str =
    include_str!("../../../scripts/managed-services/free-models-server.mjs");

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub settings: ManagedServiceSettings,
    pub runtime_installed: bool,
    pub running: bool,
    pub managed_by_other_instance: bool,
    pub port: Option<u16>,
    pub process_id: Option<u32>,
    pub available_providers: Vec<String>,
    pub revision: String,
    pub node_version: String,
    pub last_error: Option<String>,
}

/// Backend-only. Never serialize this object or expose the key to the WebView.
pub struct ModelConnection {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

struct OwnedProcess {
    child: Child,
    job: Job,
    identity: ServiceIdentity,
    _instance_lock: File,
    drain: tokio::task::JoinHandle<()>,
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.job.terminate();
        self.drain.abort();
    }
}

#[derive(Default)]
struct ServiceState {
    owned: Option<OwnedProcess>,
    last_error: Option<String>,
}

#[derive(Clone)]
pub struct ManagedServices {
    root: PathBuf,
    state: Arc<Mutex<ServiceState>>,
}

impl ManagedServices {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            state: Arc::new(Mutex::new(ServiceState::default())),
        }
    }
    fn config_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    fn identity_path(&self) -> PathBuf {
        self.root.join("instance.json")
    }
    fn runtime_path(&self) -> PathBuf {
        self.root
            .join(format!("runtime-{}", config::ROUTER_REVISION))
    }

    async fn ensure_root(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root)
            .map_err(|_| "Não foi possível criar o diretório do serviço.".to_string())?;
        #[cfg(windows)]
        {
            let sid =
                crate::platform::current_user_sid().ok_or("Usuário do serviço indisponível.")?;
            let mut command = Command::new("icacls.exe");
            command.args([
                self.root.to_string_lossy().as_ref(),
                "/inheritance:r",
                "/grant:r",
                &format!("*{sid}:(OI)(CI)F"),
                "*S-1-5-18:(OI)(CI)F",
            ]);
            command
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let result = tokio::time::timeout(Duration::from_secs(10), command.status())
                .await
                .map_err(|_| "Proteção do diretório excedeu o prazo.".to_string())?
                .map_err(|_| "Proteção do diretório indisponível.".to_string())?;
            if !result.success() {
                return Err(
                    "Não foi possível restringir o diretório do serviço ao usuário.".into(),
                );
            }
        }
        #[cfg(not(windows))]
        {
            return Err("Serviço opcional qualificado apenas no Windows.".into());
        }
        Ok(())
    }

    async fn observed_identity(&self, state: &mut ServiceState) -> Option<(ServiceIdentity, bool)> {
        if state
            .owned
            .as_mut()
            .is_some_and(|p| matches!(p.child.try_wait(), Ok(Some(_))))
        {
            state.owned.take();
        }
        let (identity, other) = match state.owned.as_ref() {
            Some(process) => (process.identity.clone(), false),
            None => (runtime::load_identity(&self.identity_path())?, true),
        };
        let token = vault::get("local-transport").ok().flatten()?;
        runtime::verify_identity(&identity, &token).await.ok()?;
        Some((identity, other))
    }

    async fn status_locked(&self, state: &mut ServiceState) -> ServiceStatus {
        let settings = match config::load(&self.config_path()) {
            Ok(settings) => settings,
            Err(error) => {
                state.last_error = Some(error);
                ManagedServiceSettings::default()
            }
        };
        let observed = self.observed_identity(state).await;
        let available_providers = config::ALLOWED_PROVIDERS
            .iter()
            .filter(|p| {
                vault::get(&format!("provider-{p}"))
                    .ok()
                    .flatten()
                    .is_some()
            })
            .map(|p| (*p).to_string())
            .collect();
        ServiceStatus {
            settings,
            runtime_installed: self.runtime_path().join("manifest.json").is_file(),
            running: observed.is_some(),
            managed_by_other_instance: observed.as_ref().is_some_and(|(_, other)| *other),
            port: observed.as_ref().map(|(i, _)| i.port),
            process_id: observed.as_ref().map(|(i, _)| i.pid),
            available_providers,
            revision: config::ROUTER_REVISION.into(),
            node_version: config::NODE_VERSION.into(),
            last_error: state.last_error.clone(),
        }
    }

    pub async fn status(&self) -> ServiceStatus {
        let mut state = self.state.lock().await;
        self.status_locked(&mut state).await
    }

    pub async fn configure(
        &self,
        settings: ManagedServiceSettings,
    ) -> Result<ServiceStatus, String> {
        settings.validate()?;
        let mut state = self.state.lock().await;
        self.ensure_root().await?;
        if self.observed_identity(&mut state).await.is_some() {
            self.stop_locked(&mut state).await?;
        }
        let previous = config::load(&self.config_path())?;
        if previous.autostart != settings.autostart {
            if settings.autostart && !self.runtime_path().join("manifest.json").is_file() {
                return Err(
                    "Instale o runtime antes de habilitar a inicialização automática.".into(),
                );
            }
            self.set_autostart(settings.autostart).await?;
        }
        if let Err(error) = config::save(&self.config_path(), &settings) {
            if previous.autostart != settings.autostart {
                let _ = self.set_autostart(previous.autostart).await;
            }
            return Err(error);
        }
        state.last_error = None;
        Ok(self.status_locked(&mut state).await)
    }

    pub async fn set_provider_key(
        &self,
        platform: String,
        value: String,
    ) -> Result<ServiceStatus, String> {
        let mut state = self.state.lock().await;
        if config::load(&self.config_path())?.setup_decision != "accepted" {
            return Err("Configure e aceite o modo opcional antes de guardar uma chave.".into());
        }
        if self.observed_identity(&mut state).await.is_some() {
            return Err("Pare o serviço antes de alterar suas credenciais.".into());
        }
        vault::set_provider(&platform, value.trim())?;
        Ok(self.status_locked(&mut state).await)
    }

    pub async fn provision(&self) -> Result<ServiceStatus, String> {
        let state = self.state.lock().await;
        if config::load(&self.config_path())?.setup_decision != "accepted" {
            return Err("O download do runtime requer aceitar a configuração opcional.".into());
        }
        self.ensure_root().await?;
        let _lock = runtime::instance_lock(&self.root.join("setup.lock"))?;
        if self.runtime_path().exists() {
            return Err(
                "Runtime já existe e foi preservado. A atualização exige uma operação explícita."
                    .into(),
            );
        }
        let scripts = self.root.join("setup");
        std::fs::create_dir_all(&scripts)
            .map_err(|_| "Não foi possível preparar o setup.".to_string())?;
        std::fs::write(scripts.join("provision-free-models.ps1"), PROVISION_SCRIPT)
            .map_err(|_| "Script de setup indisponível.".to_string())?;
        std::fs::write(scripts.join("free-models-server.mjs"), SERVER_SCRIPT)
            .map_err(|_| "Ponte de setup indisponível.".to_string())?;
        // The filesystem setup lock prevents concurrent installers. Do not
        // hold the service state mutex across downloads: status and stopping
        // existing infrastructure must remain responsive during a cold setup.
        drop(state);
        let mut command = powershell();
        command
            .args(["-File"])
            .arg(scripts.join("provision-free-models.ps1"))
            .arg("-Destination")
            .arg(self.runtime_path())
            .env_clear();
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if let Some(system) = std::env::var_os("SystemRoot") {
            command.env("PATH", PathBuf::from(system).join("System32"));
        }
        command.env("TEMP", &scripts).env("TMP", &scripts);
        // No stdout/stderr retained: dependency installation may print URLs and
        // machine paths. Status stays a bounded, actionable error.
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let job = Job::new()?;
        #[cfg(windows)]
        command.creation_flags(0x08000000 | windows::Win32::System::Threading::CREATE_SUSPENDED.0);
        let mut child = command
            .spawn()
            .map_err(|_| "Não foi possível executar o setup opcional.".to_string())?;
        let pid = child
            .id()
            .ok_or("O setup não possui identificação de processo.")?;
        if let Err(error) = job.attach_and_resume(pid) {
            job.terminate();
            let _ = child.start_kill();
            return Err(error);
        }
        let result = tokio::time::timeout(Duration::from_secs(3600), child.wait())
            .await
            .map_err(|_| "O setup excedeu o prazo. Nenhum serviço foi iniciado.".to_string())?
            .map_err(|_| "Não foi possível executar o setup opcional.".to_string())?;
        if !result.success() {
            return Err(
                "O setup do runtime falhou. Os arquivos de diagnóstico foram preservados.".into(),
            );
        }
        let runtime_path = self.runtime_path();
        tokio::task::spawn_blocking(move || runtime::verify_runtime(&runtime_path))
            .await
            .map_err(|_| "Verificação do runtime interrompida.".to_string())??;
        let manifest = std::fs::read(self.runtime_path().join("manifest.json"))
            .map_err(|_| "O manifesto instalado não pôde ser confirmado.".to_string())?;
        vault::record_runtime_manifest(config::ROUTER_REVISION, &runtime::hex_digest(&manifest))?;
        let mut state = self.state.lock().await;
        Ok(self.status_locked(&mut state).await)
    }

    pub async fn start(&self) -> Result<ServiceStatus, String> {
        let mut state = self.state.lock().await;
        let result = self.start_locked(&mut state).await;
        match result {
            Ok(()) => {
                state.last_error = None;
                Ok(self.status_locked(&mut state).await)
            }
            Err(error) => {
                state.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    async fn start_locked(&self, state: &mut ServiceState) -> Result<(), String> {
        let settings = config::load(&self.config_path())?;
        if !settings.enabled || settings.setup_decision != "accepted" {
            return Err("O serviço opcional está desabilitado.".into());
        }
        if self.observed_identity(state).await.is_some() {
            return Ok(());
        }
        self.ensure_root().await?;
        let instance_lock = runtime::instance_lock(&self.root.join("instance.lock"))?;
        let runtime_path = self.runtime_path();
        let manifest = std::fs::read(runtime_path.join("manifest.json"))
            .map_err(|_| "Instale o runtime opcional antes de iniciar.".to_string())?;
        let receipt = vault::runtime_manifest_receipt(config::ROUTER_REVISION)?
            .ok_or("O recibo protegido do runtime está ausente. Reconfigure a instalação.")?;
        if receipt != runtime::hex_digest(&manifest) {
            return Err(
                "O manifesto do runtime mudou e não corresponde ao recibo protegido.".into(),
            );
        }
        let check_path = runtime_path.clone();
        let node = tokio::task::spawn_blocking(move || runtime::verify_runtime(&check_path))
            .await
            .map_err(|_| "Verificação do runtime interrompida.".to_string())??;
        let model = settings.model.ok_or("Modelo do serviço não configurado.")?;
        let mut providers = Vec::new();
        for platform in &settings.approved_providers {
            if let Some(key) = vault::get(&format!("provider-{platform}"))? {
                providers.push(json!({ "platform": platform, "key": key }));
            }
        }
        if !providers.iter().any(|p| p["platform"] == model.platform) {
            return Err("A chave do provedor selecionado está ausente.".into());
        }
        let token = vault::persistent_secret("local-transport")?;
        // Existing DB + missing encryption key must never cause silent key
        // replacement, which would make stored credentials unreadable.
        if self.root.join("data/free-models.db").exists()
            && vault::get("encryption-master")?.is_none()
        {
            return Err(
                "A chave mestra do banco está ausente. Recupere o cofre antes de iniciar.".into(),
            );
        }
        let encryption = vault::persistent_secret("encryption-master")?;
        let instance_id = vault::random_hex()?;
        let data = self.root.join("data");
        std::fs::create_dir_all(&data)
            .map_err(|_| "Diretório de dados indisponível.".to_string())?;
        let job = Job::new()?;
        let mut command = Command::new(node);
        command
            .arg(runtime_path.join("free-models-server.mjs"))
            .current_dir(&runtime_path)
            .env_clear();
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if let Some(system) = std::env::var_os("SystemRoot") {
            command.env("PATH", PathBuf::from(system).join("System32"));
        }
        command.env("TEMP", &data).env("TMP", &data).env("NODE_ENV", "production")
            .env("HOST", "127.0.0.1").env("FREEAPI_DB_PATH", data.join("free-models.db")).env("FREEAPI_DB_DIR_HARDENING", "1")
            .env("ENCRYPTION_KEY", encryption).env("COUCOU_FREE_LOCAL_KEY", &token).env("COUCOU_FREE_INSTANCE", &instance_id)
            .env("COUCOU_FREE_BOOTSTRAP", json!({"providers":providers,"model":{"platform":model.platform,"modelId":model.model_id}}).to_string())
            .env("RESPONSE_CACHE", "false").env("RESPONSE_CACHE_PERSIST", "false").env("FREELLMAPI_COMPRESSION", "off")
            .env("FREELLMAPI_CONTEXT_HANDOFF", "off").env("FREELLMAPI_UPDATE_CHECK", "off").env("MAX_CONSECUTIVE_UPSTREAM_FAILS", "1")
            .env("VALIDATE_TOOL_ARGUMENTS", "1").env("REQUEST_MAX_TOKENS_BUDGET", "32768")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000 | windows::Win32::System::Threading::CREATE_SUSPENDED.0);
        let mut child = command
            .spawn()
            .map_err(|_| "Não foi possível iniciar o runtime próprio.".to_string())?;
        let pid = child
            .id()
            .ok_or("O serviço não possui identificação de processo.")?;
        if let Err(error) = job.attach_and_resume(pid) {
            job.terminate();
            let _ = child.start_kill();
            return Err(error);
        }
        let stdout = child
            .stdout
            .take()
            .ok_or("O serviço não possui canal de prontidão.")?;
        let mut reader = BufReader::new(stdout);
        let mut bytes = Vec::new();
        let ready = tokio::time::timeout(
            Duration::from_secs(120),
            (&mut reader).take(1025).read_until(b'\n', &mut bytes),
        )
        .await;
        if !matches!(ready, Ok(Ok(n)) if n > 0 && n <= 1024 && bytes.last() == Some(&b'\n')) {
            job.terminate();
            let _ = child.wait().await;
            return Err(
                "O servidor não ficou pronto no prazo; ele foi encerrado sem consultar modelos."
                    .into(),
            );
        }
        let identity: ServiceIdentity = serde_json::from_slice(&bytes)
            .map_err(|_| "Prontidão do servidor inválida.".to_string())?;
        if identity.pid != pid || identity.instance_id != instance_id {
            job.terminate();
            return Err("Prontidão pertence a outro processo.".into());
        }
        runtime::verify_identity(&identity, &token).await?;
        std::fs::write(
            self.identity_path(),
            serde_json::to_vec(&identity)
                .map_err(|_| "Identidade do serviço inválida.".to_string())?,
        )
        .map_err(|_| "Não foi possível registrar a instância própria.".to_string())?;
        let drain = tokio::spawn(async move {
            let mut sink = tokio::io::sink();
            let _ = tokio::io::copy(&mut reader, &mut sink).await;
        });
        state.owned = Some(OwnedProcess {
            child,
            job,
            identity,
            _instance_lock: instance_lock,
            drain,
        });
        Ok(())
    }

    async fn stop_locked(&self, state: &mut ServiceState) -> Result<(), String> {
        if let Some(mut process) = state.owned.take() {
            process.job.terminate();
            tokio::time::timeout(Duration::from_secs(5), process.child.wait())
                .await
                .map_err(|_| {
                    "A parada não foi confirmada no prazo; nenhum novo serviço foi iniciado."
                        .to_string()
                })?
                .map_err(|_| "A parada teve resultado incerto; não será repetida.".to_string())?;
        } else if let Some(identity) = runtime::load_identity(&self.identity_path()) {
            if let Some(token) = vault::get("local-transport")? {
                // An uncertain/foreign endpoint is never sent credentials or
                // killed by PID. Only a proven service accepts this stop call.
                if let Ok(client) = runtime::verify_identity(&identity, &token).await {
                    let response = client
                        .post(format!("http://127.0.0.1:{}/_coucou/stop", identity.port))
                        .bearer_auth(&token)
                        .send()
                        .await
                        .map_err(|_| {
                            "A parada do serviço teve resultado incerto; não será repetida."
                                .to_string()
                        })?;
                    if !response.status().is_success() {
                        return Err("O serviço recusou a parada; ela não será repetida.".into());
                    }
                    // An acknowledgment is not process termination. Send the
                    // stop once, then only observe disappearance before changing
                    // configuration or reporting that the service is stopped.
                    let confirmed = tokio::time::timeout(Duration::from_secs(6), async {
                        loop {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            if runtime::verify_identity(&identity, &token).await.is_err() {
                                break;
                            }
                        }
                    })
                    .await;
                    if confirmed.is_err() {
                        return Err("O serviço recebeu a parada, mas o término não foi confirmado. A solicitação não será repetida.".into());
                    }
                }
            }
        }
        // Metadata can remain after a crash. It has no secret and is never
        // trusted without the challenge, so a stale file does not block setup.
        state.last_error = None;
        Ok(())
    }

    pub async fn stop(&self) -> Result<ServiceStatus, String> {
        let mut state = self.state.lock().await;
        self.stop_locked(&mut state).await?;
        Ok(self.status_locked(&mut state).await)
    }

    pub async fn model_connection(&self) -> Result<ModelConnection, String> {
        let mut state = self.state.lock().await;
        let settings = config::load(&self.config_path())?;
        if !settings.enabled {
            return Err("Modelos opcionais desabilitados.".into());
        }
        let model = settings.model.ok_or("Modelo não configurado.")?;
        let (identity, _) = self
            .observed_identity(&mut state)
            .await
            .ok_or("O serviço local está parado ou não foi verificado.")?;
        let token = vault::get("local-transport")?.ok_or("Credencial local indisponível.")?;
        Ok(ModelConnection {
            base_url: format!("http://127.0.0.1:{}/v1", identity.port),
            api_key: token,
            model: format!("{}/{}", model.platform, model.model_id),
        })
    }

    async fn set_autostart(&self, enabled: bool) -> Result<(), String> {
        let script = self.root.join("autostart-free-models.ps1");
        std::fs::write(&script, AUTOSTART_SCRIPT)
            .map_err(|_| "Não foi possível preparar a inicialização do serviço.".to_string())?;
        let executable = std::env::current_exe()
            .map_err(|_| "Executável do Coucou indisponível.".to_string())?;
        let result = powershell()
            .arg("-File")
            .arg(script)
            .arg("-Executable")
            .arg(executable)
            .arg("-Action")
            .arg(if enabled { "enable" } else { "disable" })
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|_| "A inicialização do serviço não pôde ser alterada.".to_string())?;
        if !result.success() {
            return Err("O registro de inicialização não foi alterado; uma entrada de outra instalação será preservada.".into());
        }
        Ok(())
    }
}

fn powershell() -> Command {
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive"]);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
}

/// Called before Tauri/single-instance handling only for the explicit service
/// launch argument. No UI, model inference, task resume or message is started.
pub async fn headless_entry() -> Result<(), String> {
    let manager =
        ManagedServices::new(crate::settings::local_dir().join("managed-services/free-models"));
    let settings = config::load(&manager.config_path())?;
    if !settings.enabled || !settings.autostart || settings.setup_decision != "accepted" {
        return Ok(());
    }
    manager.start().await?;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut state = manager.state.lock().await;
        let Some(process) = state.owned.as_mut() else {
            break;
        };
        if process
            .child
            .try_wait()
            .map_err(|_| "Estado do serviço indisponível.".to_string())?
            .is_some()
        {
            break;
        }
    }
    Ok(())
}
