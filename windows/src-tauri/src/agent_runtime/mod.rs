//! Coucou Agent Runtime: a closed, typed registry. Model text is never a command.
mod plan;
#[cfg(windows)]
mod windows;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(tag = "name", content = "arguments", deny_unknown_fields)]
pub enum ToolCall {
    #[serde(rename = "coucou_open_device_manager")]
    DeviceManager(Empty),
    #[serde(rename = "coucou_open_windows_settings")]
    Settings(SettingsArgs),
    #[serde(rename = "coucou_open_application")]
    Application(ApplicationArgs),
    #[serde(rename = "coucou_open_directory")]
    Directory(PathArgs),
    #[serde(rename = "coucou_query_processes")]
    Processes(Empty),
    #[serde(rename = "coucou_organize_files_dry_run")]
    Organize(PathArgs),
    #[serde(rename = "coucou_query_recycle_bin")]
    RecycleBin(Empty),
    #[serde(rename = "coucou_cli_version")]
    CliVersion(CliArgs),
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
    pub path: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsArgs {
    pub page: SettingsPage,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsPage {
    Home,
    Display,
    Bluetooth,
    WindowsUpdate,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationArgs {
    pub application: Application,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Application {
    Notepad,
    Calculator,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliArgs {
    pub provider: Provider,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Codex,
    Claude,
    Gemini,
}
impl Provider {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }
}

pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}
pub struct ToolRegistry {
    definitions: Vec<ToolDefinition>,
}
impl Default for ToolRegistry {
    fn default() -> Self {
        let empty = json!({"type":"object","properties":{},"additionalProperties":false});
        let path = json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096}},"required":["path"],"additionalProperties":false});
        let choice = |key: &str, values: Value| json!({"type":"object","properties":{key:{"type":"string","enum":values}},"required":[key],"additionalProperties":false});
        Self { definitions: vec![
            ToolDefinition { name:"coucou_open_device_manager", description:"Abre o Gerenciador de Dispositivos após confirmação única. Não altera dispositivos.", schema:empty.clone() },
            ToolDefinition { name:"coucou_open_windows_settings", description:"Abre uma página autorizada das configurações Windows após confirmação única. Não altera configurações.", schema:choice("page",json!(["home","display","bluetooth","windows_update"])) },
            ToolDefinition { name:"coucou_open_application", description:"Abre somente Bloco de Notas ou Calculadora, sem argumentos, após confirmação única.", schema:choice("application",json!(["notepad","calculator"])) },
            ToolDefinition { name:"coucou_open_directory", description:"Abre uma pasta local existente no Explorador após confirmação única. Não executa arquivos.", schema:path.clone() },
            ToolDefinition { name:"coucou_query_processes", description:"Consulta até 500 processos: PID, PID pai e nome. Não retorna linha de comando nem credenciais. Exige confirmação única.", schema:empty.clone() },
            ToolDefinition { name:"coucou_organize_files_dry_run", description:"Simula organização por categoria de até 1000 arquivos diretamente nesta pasta. Reporta conflitos sem criar pastas, mover ou excluir arquivos. Exige confirmação única.", schema:path },
            ToolDefinition { name:"coucou_query_recycle_bin", description:"Consulta até 100 nomes da lixeira do usuário, sem restaurar ou excluir. Exige confirmação única.", schema:empty },
            ToolDefinition { name:"coucou_cli_version", description:"Executa apenas --version na CLI instalada Codex, Claude ou Gemini. Sem prompt, retomada ou comandos livres; confirmação única, timeout de 10s e cancelamento.", schema:choice("provider",json!(["codex","claude","gemini"])) },
        ] }
    }
}
impl ToolRegistry {
    pub fn contains(&self, name: &str) -> bool {
        self.definitions.iter().any(|d| d.name == name)
    }
    pub fn specs(&self) -> Value {
        Value::Array(self.definitions.iter().map(|d| json!({"type":"function","name":d.name,"description":d.description,"inputSchema":d.schema})).collect())
    }
    pub fn validate(&self, name: &str, arguments: Value) -> Result<ToolCall, String> {
        if !self.contains(name) {
            return Err("Ferramenta fora do registro autorizado.".into());
        }
        let call: ToolCall = serde_json::from_value(json!({"name":name,"arguments":arguments}))
            .map_err(|_| "Parâmetros inválidos para a ferramenta registrada.".to_string())?;
        if let ToolCall::Directory(ref a) | ToolCall::Organize(ref a) = call {
            if a.path.is_empty() || a.path.len() > 4096 || a.path.chars().any(char::is_control) {
                return Err("Caminho inválido.".into());
            }
        }
        Ok(call)
    }
}

pub fn specs() -> Value {
    if cfg!(windows) {
        ToolRegistry::default().specs()
    } else {
        json!([])
    }
}
#[cfg(windows)]
pub(crate) use windows::execute;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_rejects_shell_unknown_fields_and_invalid_enums() {
        let r = ToolRegistry::default();
        for (name, args) in [
            ("shell", json!({"command":"calc"})),
            ("coucou_open_device_manager", json!({"command":"calc"})),
            (
                "coucou_open_application",
                json!({"application":"powershell"}),
            ),
            (
                "coucou_open_windows_settings",
                json!({"page":"ms-settings:anything"}),
            ),
            (
                "coucou_cli_version",
                json!({"provider":"codex","args":["exec","bad"]}),
            ),
            (
                "coucou_organize_files_dry_run",
                json!({"path":"C:\\Downloads","dryRun":false}),
            ),
            ("coucou_open_directory", json!({"path":"x\u{0000}"})),
        ] {
            assert!(r.validate(name, args).is_err(), "{name}");
        }
    }
    #[test]
    fn every_registered_schema_has_a_typed_handler() {
        let r = ToolRegistry::default();
        for spec in r.specs().as_array().unwrap() {
            let name = spec["name"].as_str().unwrap();
            let args = match name {
                "coucou_open_windows_settings" => json!({"page":"home"}),
                "coucou_open_application" => json!({"application":"notepad"}),
                "coucou_open_directory" | "coucou_organize_files_dry_run" => {
                    json!({"path":"C:\\Users"})
                }
                "coucou_cli_version" => json!({"provider":"codex"}),
                _ => json!({}),
            };
            assert!(r.validate(name, args).is_ok());
            assert_eq!(spec["inputSchema"]["additionalProperties"], false);
        }
    }
}
