use super::{
    BridgeSpec, LaunchSpec, ModelConnection, RuntimeConfig, RuntimeStatus, HERMES_REVISION,
    MIN_CONTEXT_LENGTH,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const BOOTSTRAP: &str = include_str!("bootstrap.py");
const MAX_CONFIG_BYTES: usize = 128 * 1024;
const MAX_SEED_BYTES: usize = 48 * 1024;
static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);

// The archive is pinned separately by the installer. These checks also refuse
// a manually selected incompatible source tree at the runtime boundary.
const PINNED_FILES: [(&str, &str); 36] = [
    (
        "acp_adapter/entry.py",
        "32cb6a33fc83c8925ce0c8a1aa526ef6618cb37d27832710fe2a7e9a79dfe7ee",
    ),
    (
        "acp_adapter/server.py",
        "bee679ec8d5fee13b27ad438c2799574bdf4ea214a78457147740e4ca220ec93",
    ),
    (
        "acp_adapter/session.py",
        "45e4b55f59f426aa9db0013442647b0521d664306d956b7556431e2f80cd3fc7",
    ),
    (
        "run_agent.py",
        "2dda8e9bb530d8da54cfb1625f624a5b9533300f1be828b689cfd5e73bbae373",
    ),
    (
        "model_tools.py",
        "ba97dd616fac870af2fb21823fb0e18f71ca4444abffd8361fa9a05f02518b30",
    ),
    (
        "tools/tool_search.py",
        "1b189c8209d51374bda6da3e9610f5634f43fa49bc3fbc614cf9076dec0573a9",
    ),
    (
        "agent/title_generator.py",
        "a749a64dd55f777fd4d0236429d7f89f8d7c28434727efdde240d5fe376673ee",
    ),
    (
        "hermes_cli/managed_scope.py",
        "830483c7cf2267bc30ae1e09910a1b768536e7ad5418b73dd8303e5e27ac6448",
    ),
    (
        "agent/auxiliary_client.py",
        "3e0f471b92357dd2e945603c2460ee52959cba6b5670e4c10b93689776035644",
    ),
    (
        "agent/turn_context.py",
        "1db15cc5ee39ab3dafe9f646680f8468dd2bd6401bf4123e891697bf8206efba",
    ),
    (
        "agent/client_lifecycle.py",
        "5333b2dd5c96427996f060e9c01f6ca86d882cb89a59f60482e2281de8b66d5f",
    ),
    (
        "agent/chat_completion_helpers.py",
        "15c2aa0e7863495546ad1bbaf8ad9c67de12fdbc65f10b91c7f51a210d0a5214",
    ),
    (
        "providers/__init__.py",
        "3c5013938b0a8b186199d9a096b4e0c698b6bd12cf5623bc2ffa0673b5067645",
    ),
    (
        "toolsets.py",
        "e6f2b376fa4da1313c719c21f2c96806b6b9ddc88fad7c3cef775f348f107c77",
    ),
    (
        "hermes_cli/tools_config.py",
        "204f7d689f1b303971b19dc91161acc0f31bf98a3b89cc97b1ced6731158d71b",
    ),
    (
        "hermes_cli/plugins_discovery.py",
        "4abc9492d07b5c1ea6d8ee2f174bf95e21fcc39950d4c3ab3aade652f07b3280",
    ),
    (
        "hermes_cli/env_loader.py",
        "fd7f5debf99701e153f77c21e433da280316a0846c4314023eda4aeb9fab5ecd",
    ),
    (
        "hermes_bootstrap.py",
        "89bb25daf729f1342166594517480112aba3e27a62e613d2057e3daba1848efc",
    ),
    (
        "agent/agent_init.py",
        "6fa87cac227a86b70770e6c51220e763c048e6d0f28e127cdd71e259085aaf7f",
    ),
    (
        "agent/tool_executor.py",
        "c2c65ba2d5d0847f968321e0438f2dd3c87973cb2680acb5cd52192f48b602ab",
    ),
    (
        "agent/turn_tool_round.py",
        "8afef412460a4a6a0d3df0ec219919a736ba376f4add81864fd49c0e758c531f",
    ),
    (
        "agent/turn_tool_validation.py",
        "e279a7ee1afd97b51e7d7b4cc393a2b164bd1f535fcd61e7921a65ea49f356ac",
    ),
    (
        "agent/conversation_loop.py",
        "8b8768c965579121d836d736134661433b1e672f352971fe72b950986d9baca7",
    ),
    (
        "agent/model_metadata.py",
        "051d4dde2d4e92aa6f96411df37b7ee25b9308f881be5bbaa4810cd02fd532e0",
    ),
    (
        "agent/turn_request_assembly.py",
        "c1f003a1e2a4a7645fb1f83cbed3c74a9c3d75164f9e56f15abe0a21b25a5fb4",
    ),
    (
        "hermes_cli/moa_config.py",
        "5e49772a670131876d430f605b62d6bfb51cacc0dc91726013c77f7c597d7ebc",
    ),
    (
        "agent/tool_dispatch_helpers.py",
        "ebcfe2d9635cab078c2eb71f563fc8bc9931880092b6090fcf84aa3f81c22675",
    ),
    (
        "tools/registry.py",
        "597777c40320f90f5ca62f10d74a3112061193d2c712b752fd9d7300350f3c4b",
    ),
    (
        "tools/mcp_tool_discovery.py",
        "01d977547d3871bd1e9dd2b2ae5af4fa3212e6e126c8eb68ed770329ee39e83d",
    ),
    (
        "tools/mcp_tool_transport.py",
        "c384d222f26ed8e27fbf7055d02c0a5fe503fa49e7c9ce96f715bfb9f2c28265",
    ),
    (
        "hermes_cli/plugins.py",
        "b0f09d0004890ff0b2c07d17ee7c5688709162da4fd43d404cabbe13f1d9f59d",
    ),
    (
        "hermes_cli/mcp_startup.py",
        "c44f0fa67e3159adc6484df673f0a8de22cc4e6f787e3126d97a0b849379ed65",
    ),
    (
        "acp_adapter/commands.py",
        "01714ba7501ccb57ac1d8548b527c7af1a7debf1d99b196ec89fa842c70772c0",
    ),
    (
        "hermes_cli/config.py",
        "500b630e0c61f8a95150f46ff0be094f2f141a188ebe00872e4b42217d21ece8",
    ),
    (
        "hermes_cli/runtime_provider.py",
        "6fa7cf555a6924fabac01212fdc38a88d1bae951b6a3a7be2ee85b9418020dda",
    ),
    (
        "acp_adapter/events.py",
        "edf3b8b3679f8638e7c6a43018e906f73e3a672afe97708c0b2008055033c7d0",
    ),
];

fn normalized(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(tail) = text.strip_prefix("\\\\?\\UNC\\") {
        return PathBuf::from(format!("\\\\{tail}"));
    }
    if let Some(tail) = text.strip_prefix("\\\\?\\") {
        if tail.as_bytes().get(1) == Some(&b':') {
            return PathBuf::from(tail);
        }
    }
    path.to_path_buf()
}

fn reject_links(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("O caminho do runtime Hermes deve ser absoluto.".into());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err("O perfil Hermes não aceita caminhos com '..'.".into());
        }
        current.push(component.as_os_str());
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            continue;
        };
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let linked = metadata.file_type().is_symlink();
        if linked {
            return Err("O runtime ou perfil Hermes contém um link/reparse não permitido.".into());
        }
    }
    Ok(())
}

fn validate_runtime(config: &RuntimeConfig) -> Result<(PathBuf, PathBuf), String> {
    if config.python_executable.trim().is_empty() || config.source_directory.trim().is_empty() {
        return Err(
            "Configure o Python e a instalação gerenciada do Hermes antes de conversar.".into(),
        );
    }
    let python = Path::new(&config.python_executable);
    let source = Path::new(&config.source_directory);
    reject_links(python)?;
    reject_links(source)?;
    if !python.is_file() || !source.is_dir() {
        return Err("O Python ou a pasta de origem do Hermes não está disponível.".into());
    }
    #[cfg(windows)]
    if !python
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err("Hermes exige python.exe nativo; scripts e shims não são executados.".into());
    }
    for (relative, expected) in PINNED_FILES {
        let file = source.join(relative);
        reject_links(&file)?;
        let bytes =
            fs::read(file).map_err(|_| format!("Runtime Hermes incompleto: {relative}."))?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("Um arquivo do runtime Hermes excede o limite de validação.".into());
        }
        // Git on Windows may check out the identical source with CRLF.
        let text = std::str::from_utf8(&bytes).map_err(|_| "Código Hermes não está em UTF-8.")?;
        let hash = format!(
            "{:x}",
            Sha256::digest(text.replace("\r\n", "\n").as_bytes())
        );
        if hash != expected {
            return Err(format!("Hermes incompatível: {relative} não corresponde à revisão {HERMES_REVISION}. O chat não foi iniciado."));
        }
    }
    Ok((
        normalized(&python.canonicalize().map_err(|_| "Python indisponível.")?),
        normalized(
            &source
                .canonicalize()
                .map_err(|_| "Origem Hermes indisponível.")?,
        ),
    ))
}

pub fn runtime_status(config: &RuntimeConfig) -> RuntimeStatus {
    match validate_runtime(config) {
        Ok((python, source)) => RuntimeStatus {
            available: true,
            revision: HERMES_REVISION,
            python_executable: Some(python.to_string_lossy().into_owned()),
            source_directory: Some(source.to_string_lossy().into_owned()),
            detail: "Origem Hermes fixada e conferida. A conexão e as ferramentas serão verificadas antes de cada turno.".into(),
            live_isolation_qualified: false,
        },
        Err(detail) => RuntimeStatus {
            available: false, revision: HERMES_REVISION,
            python_executable: None, source_directory: None, detail,
            live_isolation_qualified: false,
        },
    }
}

pub(super) fn validate_model(model: &ModelConnection) -> Result<(), String> {
    let url =
        reqwest::Url::parse(&model.base_url).map_err(|_| "Endereço do modelo Hermes inválido.")?;
    let loopback = matches!(
        url.host_str(),
        Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
    );
    if (!matches!(url.scheme(), "https") && !(url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().trim_end_matches('/').ends_with("/v1")
    {
        return Err("Hermes exige HTTPS ou loopback local e endpoint terminado em /v1, sem credenciais na URL.".into());
    }
    if model.model.trim().is_empty()
        || model.model.len() > 256
        || model.model.chars().any(char::is_control)
        || model.api_key.trim().is_empty()
        || model.api_key.len() > 8192
        || model.api_key.chars().any(char::is_control)
        || !(MIN_CONTEXT_LENGTH..=1_000_000).contains(&model.context_length)
    {
        return Err(
            "Conexão do Hermes incompleta: configure modelo, chave e contexto entre 64.000 e 1.000.000 tokens compatível com o provedor."
                .into(),
        );
    }
    Ok(())
}

fn validate_bridge(bridge: &BridgeSpec) -> Result<(), String> {
    let current = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|_| "Não foi possível identificar a ponte Coucou.")?;
    let command = bridge
        .command
        .canonicalize()
        .map_err(|_| "Executável da ponte Coucou indisponível.")?;
    if command != current || bridge.args != ["--hermes-tool-bridge"] {
        return Err(
            "O Hermes aceita somente a ponte de ferramentas do executável Coucou atual.".into(),
        );
    }
    for (key, value) in &bridge.environment {
        if !key.starts_with("COUCOU_")
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
            || value.len() > 16 * 1024
            || value.chars().any(char::is_control)
        {
            return Err("Ambiente da ponte de ferramentas inválido.".into());
        }
    }
    Ok(())
}

fn write_owned(path: &Path, bytes: &[u8]) -> Result<(), String> {
    reject_links(path)?;
    let parent = path.parent().ok_or("Perfil Hermes inválido.")?;
    let temp = parent.join(format!(
        ".coucou-write-{}-{}",
        std::process::id(),
        NEXT_WRITE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|_| "Não foi possível preparar o perfil Hermes.")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Não foi possível salvar o perfil Hermes.")?;
        fs::rename(&temp, path)
            .map_err(|_| "Não foi possível atualizar o perfil Hermes.".to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub(super) fn managed_profile(root: &Path) -> Result<PathBuf, String> {
    reject_links(root)?;
    if root.exists() {
        let marker = root.join("coucou-managed.json");
        let nonempty = fs::read_dir(root)
            .map_err(|_| "Perfil Hermes indisponível.")?
            .next()
            .is_some();
        if nonempty && !marker.is_file() {
            return Err(
                "Esta pasta Hermes não pertence ao Coucou. O perfil existente foi preservado."
                    .into(),
            );
        }
        if marker.is_file() {
            reject_links(&marker)?;
            let value: Value = serde_json::from_slice(
                &fs::read(marker).map_err(|_| "Marca do perfil indisponível.")?,
            )
            .map_err(|_| "Marca do perfil Hermes inválida.")?;
            if value.get("owner").and_then(Value::as_str) != Some("coucou") {
                return Err("Esta pasta Hermes não pertence ao Coucou.".into());
            }
        }
    }
    crate::platform::ensure_private_dir(root)
        .map_err(|_| "Não foi possível criar o perfil exclusivo Hermes.")?;
    for relative in [
        "workspace",
        "home",
        "cache/scratch",
        "memories",
        "skills",
        "isolated-appdata",
        "isolated-localappdata",
    ] {
        let dir = root.join(relative);
        reject_links(&dir)?;
        crate::platform::ensure_private_dir(&dir)
            .map_err(|_| "Não foi possível criar as pastas do perfil Hermes.")?;
    }
    write_owned(
        &root.join("coucou-managed.json"),
        &serde_json::to_vec(
            &json!({"owner":"coucou","revision":HERMES_REVISION,"schemaVersion":1}),
        )
        .unwrap(),
    )?;
    Ok(normalized(
        &root
            .canonicalize()
            .map_err(|_| "Perfil Hermes indisponível.")?,
    ))
}

fn profile_config(model: &ModelConnection) -> Value {
    json!({
        "model": {"provider":"custom","default":model.model,"base_url":model.base_url,
            "api_key":"${COUCOU_HERMES_MODEL_KEY}","api_mode":"chat_completions","context_length":model.context_length},
        "platform_toolsets":{"acp":[]}, "mcp_servers":{}, "plugins":{"enabled":[],"disabled":[],"auto_update_check_hours":0,"auto_apply":false},
        "agent":{"max_turns":12,"api_max_retries":1,"auto_recovery_cycles":0,"environment_probe":false,
            "disabled_toolsets":["terminal","file","skills","memory","delegate","cron","browser","web","code_execution"]},
        "context":{"engine":"builtin"},
        "memory":{"provider":"","memory_enabled":false,"user_profile_enabled":false,"nudge_interval":0,"write_approval":true},
        "skills":{"external_dirs":[],"auto_load":[],"project_dirs":[],"trusted_project_dirs":[],"inline_shell":false,"write_approval":true},
        "curator":{"enabled":false}, "checkpoints":{"enabled":false},
        "auxiliary":{"background_review":{"enabled":false},"title_generation":{"enabled":false,"model_upgrade_enabled":false}},
        "tools":{"tool_search":{"enabled":"off"}}, "fallback_model":[], "hooks":{},
        "security":{"allow_lazy_installs":false}
    })
}

pub fn prepare_launch(
    config: &RuntimeConfig,
    profile_root: &Path,
    model: &ModelConnection,
    bridge: &BridgeSpec,
) -> Result<LaunchSpec, String> {
    validate_model(model)?;
    validate_bridge(bridge)?;
    let (python, source) = validate_runtime(config)?;
    let profile_home = managed_profile(profile_root)?;
    if profile_home.starts_with(&source) || source.starts_with(&profile_home) {
        return Err("A origem do runtime e o perfil Hermes devem ser pastas separadas.".into());
    }
    let config_json = serde_json::to_vec_pretty(&profile_config(model))
        .map_err(|_| "Configuração Hermes inválida.")?;
    if config_json.len() > MAX_CONFIG_BYTES {
        return Err("Configuração Hermes excede o limite.".into());
    }
    write_owned(&profile_home.join("config.yaml"), &config_json)?;
    write_owned(
        &profile_home.join("coucou-bootstrap.py"),
        BOOTSTRAP.as_bytes(),
    )?;
    let mut environment = BTreeMap::new();
    for key in [
        "SystemRoot",
        "WINDIR",
        "SYSTEMDRIVE",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    let python_dir = python.parent().ok_or("Pasta do Python indisponível.")?;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_default();
    let paths = [
        python_dir.to_path_buf(),
        system_root.join("System32"),
        system_root,
    ];
    environment.insert(
        "PATH".into(),
        std::env::join_paths(paths)
            .map_err(|_| "PATH isolado inválido.")?
            .to_string_lossy()
            .into_owned(),
    );
    let home = profile_home.join("home").to_string_lossy().into_owned();
    for key in ["HOME", "USERPROFILE"] {
        environment.insert(key.into(), home.clone());
    }
    environment.insert(
        "APPDATA".into(),
        profile_home
            .join("isolated-appdata")
            .to_string_lossy()
            .into_owned(),
    );
    environment.insert(
        "LOCALAPPDATA".into(),
        profile_home
            .join("isolated-localappdata")
            .to_string_lossy()
            .into_owned(),
    );
    let scratch = profile_home
        .join("cache/scratch")
        .to_string_lossy()
        .into_owned();
    for key in ["TEMP", "TMP", "TMPDIR"] {
        environment.insert(key.into(), scratch.clone());
    }
    environment.insert(
        "HERMES_HOME".into(),
        profile_home.to_string_lossy().into_owned(),
    );
    environment.insert(
        "HERMES_RUNTIME_DIR".into(),
        source.to_string_lossy().into_owned(),
    );
    environment.insert("HERMES_ACP_SKIP_CONFIGURED_MCP".into(), "1".into());
    environment.insert("HERMES_ENABLE_PROJECT_PLUGINS".into(), "0".into());
    environment.insert("HERMES_DISABLE_LAZY_INSTALLS".into(), "1".into());
    environment.insert("HERMES_SUPERVISED_CHILD".into(), "1".into());
    environment.insert("PYTHONNOUSERSITE".into(), "1".into());
    environment.insert("PYTHONUTF8".into(), "1".into());
    environment.insert("COUCOU_HERMES_MODEL_KEY".into(), model.api_key.clone());
    environment.insert(
        "COUCOU_HERMES_BRIDGE_SPEC".into(),
        serde_json::to_string(bridge).map_err(|_| "Ponte Hermes inválida.")?,
    );
    Ok(LaunchSpec {
        program: python,
        arguments: vec![
            "-I".into(),
            "-B".into(),
            "-u".into(),
            profile_home
                .join("coucou-bootstrap.py")
                .to_string_lossy()
                .into_owned(),
            "--source".into(),
            source.to_string_lossy().into_owned(),
            "--profile".into(),
            profile_home.to_string_lossy().into_owned(),
        ],
        cwd: profile_home.join("workspace"),
        environment,
        profile_home,
    })
}

/// The Rust memory service remains the single writer. Only approved context is
/// mirrored here; Hermes receives it via context_read, not native memory tools.
pub fn seed_memory(profile_root: &Path, memory: &str, user: &str) -> Result<(), String> {
    if memory.len() > MAX_SEED_BYTES || user.len() > MAX_SEED_BYTES {
        return Err("O contexto Hermes excede o limite permitido.".into());
    }
    let home = managed_profile(profile_root)?;
    write_owned(&home.join("memories/MEMORY.md"), memory.as_bytes())?;
    write_owned(&home.join("memories/USER.md"), user.as_bytes())
}
