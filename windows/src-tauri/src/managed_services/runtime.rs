use super::{
    config::{NODE_VERSION, ROUTER_REVISION},
    vault,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: u32,
    revision: String,
    node_version: String,
    files: Vec<FileDigest>,
}
#[derive(Deserialize)]
struct FileDigest {
    path: String,
    sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceIdentity {
    pub protocol: u32,
    pub revision: String,
    pub instance_id: String,
    pub port: u16,
    pub pid: u32,
}

pub fn verify_runtime(root: &Path) -> Result<PathBuf, String> {
    let manifest_path = root.join("manifest.json");
    let bytes = std::fs::read(&manifest_path).map_err(|_| {
        "Runtime não instalado. Configure o componente opcional primeiro.".to_string()
    })?;
    if bytes.len() > 12 * 1024 * 1024 {
        return Err("Manifesto de runtime excede o limite.".into());
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| "Manifesto de runtime inválido.".to_string())?;
    if manifest.schema != 1
        || manifest.revision != ROUTER_REVISION
        || manifest.node_version != NODE_VERSION
        || manifest.files.is_empty()
    {
        return Err("Versão do runtime não corresponde à distribuição fixada.".into());
    }
    let canonical = root
        .canonicalize()
        .map_err(|_| "Runtime ausente.".to_string())?;
    let mut node_seen = false;
    let mut wrapper_seen = false;
    let wrapper_expected = hex_digest(include_bytes!(
        "../../../scripts/managed-services/free-models-server.mjs"
    ));
    for file in manifest.files {
        let relative = Path::new(&file.path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Um arquivo do runtime escaparia do diretório gerenciado.".into());
        }
        let path = root
            .join(relative)
            .canonicalize()
            .map_err(|_| "Runtime incompleto.".to_string())?;
        if !path.starts_with(&canonical) {
            return Err("Runtime contém um redirecionamento fora do diretório próprio.".into());
        }
        let mut reader =
            File::open(path).map_err(|_| "Arquivo do runtime indisponível.".to_string())?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 32768];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|_| "Não foi possível verificar o runtime.".to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let digest = format!("{:x}", hasher.finalize());
        if digest != file.sha256 {
            return Err("A integridade do runtime mudou. Reconfigure antes de iniciar.".into());
        }
        if file.path == "node/node.exe" {
            node_seen =
                file.sha256 == "bae898add4643fcf890a83ad8ae56e20dce7e781cab161a53991ceba70c99ffb";
        }
        if file.path == "free-models-server.mjs" {
            wrapper_seen = file.sha256 == wrapper_expected;
        }
    }
    if !node_seen || !wrapper_seen {
        return Err("O manifesto não contém o Node e a ponte oficiais da distribuição.".into());
    }
    Ok(root.join("node/node.exe"))
}

pub fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn identity_proof(secret: &str, challenge: &str) -> String {
    hex_digest(format!("{secret}{challenge}").as_bytes())
}

/// Prove local service identity before sending any Authorization header. A
/// foreign listener can receive the public challenge, never the vault token.
pub async fn verify_identity(
    identity: &ServiceIdentity,
    token: &str,
) -> Result<reqwest::Client, String> {
    if identity.protocol != 1
        || identity.revision != ROUTER_REVISION
        || identity.port == 0
        || identity.pid == 0
        || identity.instance_id.len() != 64
        || !identity.instance_id.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Identidade de serviço inválida.".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "Cliente local indisponível.".to_string())?;
    let challenge = vault::random_hex()?;
    let url = format!(
        "http://127.0.0.1:{}/_coucou/identity?challenge={challenge}",
        identity.port
    );
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "O serviço gerenciado não respondeu.".to_string())?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > 4096) {
        return Err("Resposta de identidade inválida.".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Resposta local indisponível.".to_string())?
    {
        if body.len() + chunk.len() > 4096 {
            return Err("Resposta de identidade excede o limite.".into());
        }
        body.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| "Resposta de identidade inválida.".to_string())?;
    if value["protocol"] != 1
        || value["revision"] != ROUTER_REVISION
        || value["pid"] != identity.pid
        || value["instanceId"] != identity.instance_id
        || value["proof"] != identity_proof(token, &challenge)
    {
        return Err(
            "A porta pertence a um serviço não verificado; nenhuma chave foi enviada.".into(),
        );
    }
    Ok(client)
}

pub fn load_identity(path: &Path) -> Option<ServiceIdentity> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

pub fn instance_lock(path: &Path) -> Result<File, String> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    #[cfg(not(windows))]
    {
        return Err("Gerenciamento de serviço qualificado apenas no Windows.".into());
    }
    #[cfg(windows)]
    options
        .open(path)
        .map_err(|_| "Outra instância já está gerenciando esse serviço.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_challenge_matches_node_protocol() {
        assert_eq!(
            identity_proof(&"a".repeat(64), &"b".repeat(64)),
            "fa0dafbf43f1f551e536353e9d1a942a8e86e41a0b58dfeaf264ef217f6b862a"
        );
        assert_ne!(
            identity_proof("secret", "one"),
            identity_proof("secret", "two")
        );
    }
    #[test]
    fn uninstalled_runtime_is_not_an_installed_global_node() {
        assert!(
            verify_runtime(&std::env::temp_dir().join("coucou-uninstalled-managed-runtime"))
                .is_err()
        );
    }
}
