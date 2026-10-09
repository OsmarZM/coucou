//! Separate service credentials, never borrowed from CLI/global provider keys.
use super::config::ALLOWED_PROVIDERS;
use keyring::Entry;

const SERVICE: &str = "fr.louisraille.coucou.managed-free-models";

fn entry(key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, key).map_err(|_| "O cofre protegido do sistema está indisponível.".into())
}

pub fn get(key: &str) -> Result<Option<String>, String> {
    match entry(key)?.get_password() {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Não foi possível ler o cofre protegido do serviço.".into()),
    }
}

pub fn set_provider(platform: &str, value: &str) -> Result<(), String> {
    if !ALLOWED_PROVIDERS.contains(&platform)
        || value.len() > 4096
        || value.chars().any(char::is_control)
    {
        return Err("Credencial ou provedor inválido.".into());
    }
    let target = entry(&format!("provider-{platform}"))?;
    if value.is_empty() {
        match target.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Não foi possível remover a credencial do serviço.".into()),
        }
    } else {
        target
            .set_password(value)
            .map_err(|_| "Não foi possível guardar a credencial do serviço.".into())
    }
}

pub fn persistent_secret(key: &str) -> Result<String, String> {
    if !["encryption-master", "local-transport"].contains(&key) {
        return Err("Segredo de serviço desconhecido.".into());
    }
    if let Some(secret) = get(key)? {
        if secret.len() != 64 || !secret.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(
                "Segredo protegido inválido; ele não será regenerado silenciosamente.".into(),
            );
        }
        return Ok(secret);
    }
    let value = random_hex()?;
    entry(key)?
        .set_password(&value)
        .map_err(|_| "Não foi possível guardar o segredo do serviço.".to_string())?;
    Ok(value)
}

pub fn record_runtime_manifest(revision: &str, digest: &str) -> Result<(), String> {
    if revision != super::config::ROUTER_REVISION
        || digest.len() != 64
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Recibo de integridade inválido.".into());
    }
    entry(&format!("runtime-manifest-{revision}"))?
        .set_password(digest)
        .map_err(|_| "Não foi possível guardar o recibo protegido do runtime.".into())
}

pub fn runtime_manifest_receipt(revision: &str) -> Result<Option<String>, String> {
    if revision != super::config::ROUTER_REVISION {
        return Err("Versão do recibo desconhecida.".into());
    }
    get(&format!("runtime-manifest-{revision}"))
}

pub fn random_hex() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    #[cfg(windows)]
    {
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn SystemFunction036(buffer: *mut u8, length: u32) -> u8;
        }
        if unsafe { SystemFunction036(bytes.as_mut_ptr(), bytes.len() as u32) } == 0 {
            return Err("Não foi possível criar uma identidade aleatória protegida.".into());
        }
    }
    #[cfg(not(windows))]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|_| "Entropia do sistema indisponível.".to_string())?;
    }
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
