use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ROUTER_REVISION: &str = "a6b2158c7c36ce19f888d0411846a1a0f2aa3f06";
pub const NODE_VERSION: &str = "22.22.0";
pub const ALLOWED_PROVIDERS: &[&str] = &["groq", "cerebras"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelSelection {
    pub platform: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedServiceSettings {
    pub setup_decision: String,
    pub enabled: bool,
    pub agent_name: String,
    pub autostart: bool,
    pub approved_providers: Vec<String>,
    pub model: Option<ModelSelection>,
}

impl Default for ManagedServiceSettings {
    fn default() -> Self {
        Self {
            setup_decision: "pending".into(),
            enabled: false,
            agent_name: "Hermes".into(),
            autostart: false,
            approved_providers: Vec::new(),
            model: None,
        }
    }
}

impl ManagedServiceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !["pending", "declined", "accepted"].contains(&self.setup_decision.as_str()) {
            return Err("Decisão de configuração inválida.".into());
        }
        if self.agent_name.trim().is_empty()
            || self.agent_name.chars().count() > 60
            || self.agent_name.chars().any(char::is_control)
        {
            return Err("O nome do agente deve conter entre 1 e 60 caracteres.".into());
        }
        if self.approved_providers.len() > ALLOWED_PROVIDERS.len()
            || self
                .approved_providers
                .iter()
                .any(|p| !ALLOWED_PROVIDERS.contains(&p.as_str()))
            || self
                .approved_providers
                .iter()
                .enumerate()
                .any(|(i, p)| self.approved_providers[..i].contains(p))
        {
            return Err("Provedor gratuito não permitido ou repetido.".into());
        }
        if (self.enabled || self.autostart) && self.setup_decision != "accepted" {
            return Err("Ativar o serviço exige aceitar a configuração opcional.".into());
        }
        if self.autostart && !self.enabled {
            return Err("A inicialização automática exige o serviço habilitado.".into());
        }
        if let Some(model) = &self.model {
            if !self.approved_providers.contains(&model.platform)
                || model.model_id.is_empty()
                || model.model_id.len() > 160
                || model.model_id.to_ascii_lowercase().starts_with("auto")
                || !model
                    .model_id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-._/".contains(c))
            {
                return Err("Escolha um modelo explícito de um provedor autorizado.".into());
            }
        } else if self.enabled {
            return Err("Escolha um modelo antes de habilitar o serviço.".into());
        }
        Ok(())
    }
}

pub fn load(path: &Path) -> Result<ManagedServiceSettings, String> {
    if !path.exists() {
        return Ok(ManagedServiceSettings::default());
    }
    let bytes = std::fs::read(path)
        .map_err(|_| "Não foi possível ler a configuração do serviço.".to_string())?;
    if bytes.len() > 8192 {
        return Err("Configuração do serviço excede o limite.".into());
    }
    let settings: ManagedServiceSettings = serde_json::from_slice(&bytes)
        .map_err(|_| "Configuração do serviço inválida.".to_string())?;
    settings.validate()?;
    Ok(settings)
}

pub fn save(path: &Path, settings: &ManagedServiceSettings) -> Result<(), String> {
    settings.validate()?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|_| "Configuração do serviço inválida.".to_string())?;
    let temp = path.with_extension("pending.json");
    std::fs::write(&temp, bytes)
        .map_err(|_| "Não foi possível salvar a configuração do serviço.".to_string())?;
    std::fs::rename(&temp, path)
        .map_err(|_| "Não foi possível confirmar a configuração do serviço.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn free_service_is_opt_in_and_independent() {
        let defaults = ManagedServiceSettings::default();
        assert!(defaults.validate().is_ok());
        assert!(!defaults.enabled && !defaults.autostart);
        let mut s = defaults.clone();
        s.enabled = true;
        assert!(s.validate().is_err());
        s.setup_decision = "accepted".into();
        assert!(s.validate().is_err());
        s.approved_providers = vec!["groq".into()];
        s.model = Some(ModelSelection {
            platform: "groq".into(),
            model_id: "llama-3.3-70b-versatile".into(),
        });
        assert!(s.validate().is_ok());
        s.autostart = true;
        assert!(s.validate().is_ok());
        s.enabled = false;
        assert!(s.validate().is_err());
    }
    #[test]
    fn routing_cannot_implicitly_expand_providers() {
        let mut s = ManagedServiceSettings {
            setup_decision: "accepted".into(),
            approved_providers: vec!["groq".into()],
            ..Default::default()
        };
        for id in ["auto", "AUTO", "Auto", "auto:coding", "bad\nname"] {
            s.model = Some(ModelSelection {
                platform: "groq".into(),
                model_id: id.into(),
            });
            assert!(s.validate().is_err());
        }
        s.model = Some(ModelSelection {
            platform: "cerebras".into(),
            model_id: "model".into(),
        });
        assert!(s.validate().is_err());
        s.model = None;
        s.approved_providers = vec!["paid".into()];
        assert!(s.validate().is_err());
    }
    #[test]
    fn agent_names_accept_accents_and_refuse_controls() {
        let mut s = ManagedServiceSettings {
            agent_name: "Assistente do João".into(),
            ..Default::default()
        };
        assert!(s.validate().is_ok());
        s.agent_name = "bad\nname".into();
        assert!(s.validate().is_err());
    }
}
