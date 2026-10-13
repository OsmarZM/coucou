//! User-owned supervisor configuration; credentials never enter the settings DTO.
use crate::{hermes, managed_services::ManagedServices};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSettings {
    pub enabled: bool,
    pub python_executable: String,
    pub source_directory: String,
    pub use_free_models: bool,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_context_window")]
    pub context_window_tokens: u32,
}
fn default_context_window() -> u32 {
    hermes::MIN_CONTEXT_LENGTH
}
impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            python_executable: String::new(),
            source_directory: String::new(),
            use_free_models: false,
            base_url: String::new(),
            model: String::new(),
            context_window_tokens: default_context_window(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub settings: RuntimeSettings,
    pub runtime_installed: bool,
    pub model_configured: bool,
    pub available: bool,
    pub detail: String,
    pub revision: String,
}
#[derive(Clone)]
pub struct Supervisor {
    root: PathBuf,
    setup: Arc<tokio::sync::Mutex<()>>,
}
impl Supervisor {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            setup: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
    pub fn load(&self) -> Result<RuntimeSettings, String> {
        let file = self.root.join("settings.json");
        if !file.exists() {
            return Ok(RuntimeSettings {
                python_executable: self
                    .root
                    .join("runtime/source/.venv/Scripts/python.exe")
                    .to_string_lossy()
                    .into(),
                source_directory: self.root.join("runtime/source").to_string_lossy().into(),
                use_free_models: true,
                ..Default::default()
            });
        }
        serde_json::from_slice(
            &std::fs::read(file).map_err(|_| "Configuração Hermes indisponível.")?,
        )
        .map_err(|_| "Configuração Hermes inválida.".into())
    }
    pub fn validate_settings(settings: &RuntimeSettings) -> Result<(), String> {
        validate_context_window(settings.context_window_tokens)?;
        for path in [&settings.python_executable, &settings.source_directory] {
            if path.len() > 4096
                || path.chars().any(char::is_control)
                || (!path.is_empty() && !PathBuf::from(path).is_absolute())
            {
                return Err("Informe caminhos absolutos válidos do runtime Hermes.".into());
            }
        }
        if settings.model.len() > 160 || settings.model.chars().any(char::is_control) {
            return Err("Identificador do modelo inválido.".into());
        }
        if settings.enabled && !settings.use_free_models {
            validate_endpoint(&settings.base_url)?;
            if settings.model.trim().is_empty() {
                return Err("Escolha o modelo do supervisor.".into());
            }
        }
        Ok(())
    }
    pub fn configure(&self, settings: &RuntimeSettings) -> Result<(), String> {
        Self::validate_settings(settings)?;
        std::fs::create_dir_all(&self.root)
            .map_err(|_| "Não foi possível criar a configuração Hermes.")?;
        let tmp = self.root.join("settings.pending.json");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(settings).map_err(|_| "Configuração inválida.")?,
        )
        .map_err(|_| "Não foi possível salvar Hermes.")?;
        std::fs::rename(tmp, self.root.join("settings.json"))
            .map_err(|_| "Não foi possível concluir a configuração Hermes.".into())
    }
    pub async fn connection(
        &self,
        services: &ManagedServices,
    ) -> Result<hermes::ModelConnection, String> {
        let settings = self.load()?;
        if !settings.enabled {
            return Err("Ative o supervisor Hermes nas configurações.".into());
        }
        validate_context_window(settings.context_window_tokens)?;
        if settings.use_free_models {
            let connection = services.model_connection().await?;
            Ok(hermes::ModelConnection {
                base_url: connection.base_url,
                model: connection.model,
                api_key: connection.api_key,
                context_length: settings.context_window_tokens,
            })
        } else {
            validate_endpoint(&settings.base_url)?;
            Ok(hermes::ModelConnection {
                base_url: settings.base_url,
                model: settings.model,
                api_key: crate::secrets::get("hermes-api-key")
                    .ok_or("Salve a chave do modelo Hermes nas configurações.")?,
                context_length: settings.context_window_tokens,
            })
        }
    }
    pub fn runtime_config(&self) -> Result<hermes::RuntimeConfig, String> {
        let settings = self.load()?;
        Ok(hermes::RuntimeConfig {
            python_executable: settings.python_executable,
            source_directory: settings.source_directory,
        })
    }
    pub fn profiles_root(&self) -> PathBuf {
        self.root.join("profiles")
    }
    pub async fn provision(&self) -> Result<(), String> {
        let _setup = self
            .setup
            .try_lock()
            .map_err(|_| "A instalação do Hermes já está em andamento.")?;
        crate::platform::ensure_private_dir(&self.root)
            .map_err(|_| "Diretório privado do Hermes indisponível.")?;
        let destination = self.root.join("runtime");
        if destination.exists() {
            return Err("A instalação existente foi preservada. Confira seu estado antes de tentar novamente.".into());
        }
        let script = self.root.join("provision-hermes.ps1");
        std::fs::write(
            &script,
            include_str!("../../scripts/managed-services/provision-hermes.ps1"),
        )
        .map_err(|_| "Script de instalação indisponível.")?;
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").ok_or("Windows indisponível.")?)
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let job = crate::agent_chat::process::Job::new()?;
        let mut command = tokio::process::Command::new(powershell);
        command.env_clear();
        for name in [
            "SystemRoot",
            "WINDIR",
            "SYSTEMDRIVE",
            "TEMP",
            "TMP",
            "NUMBER_OF_PROCESSORS",
            "PROCESSOR_ARCHITECTURE",
        ] {
            if let Ok(value) = std::env::var(name) {
                command.env(name, value);
            }
        }
        command.env(
            "PATH",
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_default()).join("System32"),
        );
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .arg("-Destination")
            .arg(&destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .creation_flags(0x08000000 | 0x00000004);
        let mut child = command
            .spawn()
            .map_err(|_| "Não foi possível iniciar a instalação do Hermes.")?;
        if let Err(error) =
            job.attach_and_resume(child.id().ok_or("Instalador sem identificação.")?)
        {
            job.terminate();
            let _ = child.start_kill();
            return Err(error);
        }
        let result = tokio::time::timeout(Duration::from_secs(3600), child.wait()).await;
        job.terminate();
        let result=result.map_err(|_|"A instalação do Hermes excedeu o prazo. A instalação parcial e os logs foram preservados; não houve nova tentativa.".to_string())?.map_err(|_|"Instalador interrompido.".to_string())?;
        if !result.success() {
            return Err("A instalação do Hermes falhou. Confira os logs da pasta de runtime; nenhuma conta ou modelo foi utilizado.".into());
        }
        let mut settings = self.load()?;
        settings.python_executable = destination
            .join("source/.venv/Scripts/python.exe")
            .to_string_lossy()
            .into();
        settings.source_directory = destination.join("source").to_string_lossy().into();
        if !hermes::runtime_status(&hermes::RuntimeConfig {
            python_executable: settings.python_executable.clone(),
            source_directory: settings.source_directory.clone(),
        })
        .available
        {
            return Err("A origem instalada do Hermes não passou na verificação fixada.".into());
        }
        self.configure(&settings)
    }
    pub async fn status(&self, services: &ManagedServices) -> Result<RuntimeStatus, String> {
        let settings = self.load()?;
        let runtime = hermes::runtime_status(&self.runtime_config()?);
        let runtime_installed = runtime.available;
        let model_configured = validate_context_window(settings.context_window_tokens).is_ok()
            && if settings.use_free_models {
                let state = services.status().await;
                state.running && state.settings.enabled && state.settings.model.is_some()
            } else {
                !settings.model.is_empty()
                    && validate_endpoint(&settings.base_url).is_ok()
                    && crate::secrets::present("hermes-api-key")
            };
        let available = settings.enabled && runtime_installed && model_configured;
        Ok(RuntimeStatus {
            settings,
            runtime_installed,
            model_configured,
            available,
            detail: if !runtime_installed {
                runtime.detail
            } else if available {
                "Hermes configurado. O isolamento efetivo e a conexão são verificados ao iniciar cada turno.".into()
            } else {
                "Configure a conexão do modelo e ative o supervisor Hermes.".into()
            },
            revision: hermes::HERMES_REVISION.into(),
        })
    }
}
fn validate_context_window(tokens: u32) -> Result<(), String> {
    if !(hermes::MIN_CONTEXT_LENGTH..=1_000_000).contains(&tokens) {
        return Err("O Hermes exige um modelo com contexto de pelo menos 64.000 tokens. Informe a capacidade real do modelo, até 1.000.000 tokens.".into());
    }
    Ok(())
}
fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "URL do modelo inválida.")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().trim_end_matches('/').ends_with("/v1")
        || !(url.scheme() == "https"
            || (url.scheme() == "http"
                && matches!(
                    url.host_str(),
                    Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
                )))
    {
        return Err(
            "Use HTTPS ou loopback local e endereço terminado em /v1, sem credenciais na URL."
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_credentials_and_remote_http_are_rejected() {
        assert!(validate_endpoint("https://example.com/v1").is_ok());
        assert!(validate_endpoint("http://127.0.0.1:4210/v1").is_ok());
        assert!(validate_endpoint("https://example.com/api/v1/").is_ok());
        assert!(validate_endpoint("http://[::1]:4210/v1").is_ok());
        for url in [
            "http://example.com/v1",
            "https://example.com",
            "https://example.com/v2",
            "https://user:secret@example.com",
            "https://example.com?api_key=secret",
        ] {
            assert!(validate_endpoint(url).is_err());
        }
    }
    #[test]
    fn legacy_settings_receive_minimum_and_invalid_context_is_rejected() {
        let legacy = serde_json::json!({
            "enabled": false, "pythonExecutable": "", "sourceDirectory": "",
            "useFreeModels": false, "baseUrl": "", "model": ""
        });
        let settings: RuntimeSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(settings.context_window_tokens, 64_000);
        assert!(validate_context_window(64_000).is_ok());
        assert!(validate_context_window(1_000_000).is_ok());
        for value in [0, 32_768, 63_999, 1_000_001] {
            assert!(validate_context_window(value).is_err());
        }
    }
}
