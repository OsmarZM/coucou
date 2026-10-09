use super::{
    acp, profile, ModelConnection, EFFECTIVE_TOOL_NAMES, HERMES_REVISION, MIN_CONTEXT_LENGTH,
};
use serde_json::{json, Value};

fn attestation(phase: &str) -> Value {
    json!({"_meta":{"coucouIsolation":{"schemaVersion":1,"revision":HERMES_REVISION,
        "phase":phase,"nativeToolsDisabled":true,"inheritedConfigurationDisabled":true,
        "toolNames":EFFECTIVE_TOOL_NAMES}}})
}

#[test]
fn live_attestation_rejects_missing_duplicate_extra_and_changed_runtime() {
    assert!(acp::validate_manifest(&attestation("ready"), "ready").is_ok());
    assert!(acp::validate_manifest(&json!({}), "ready").is_err());
    assert!(acp::validate_manifest(&attestation("policy"), "ready").is_err());
    let mut changed = attestation("ready");
    changed["_meta"]["coucouIsolation"]["revision"] = json!("another-revision");
    assert!(acp::validate_manifest(&changed, "ready").is_err());
    changed = attestation("ready");
    changed["_meta"]["coucouIsolation"]["toolNames"][0] = json!("terminal");
    assert!(acp::validate_manifest(&changed, "ready").is_err());
    changed = attestation("ready");
    changed["_meta"]["coucouIsolation"]["toolNames"][0] =
        changed["_meta"]["coucouIsolation"]["toolNames"][1].clone();
    assert!(acp::validate_manifest(&changed, "ready").is_err());
    changed = attestation("ready");
    changed["_meta"]["coucouIsolation"]["toolNames"]
        .as_array_mut()
        .unwrap()
        .push(json!("read_file"));
    assert!(acp::validate_manifest(&changed, "ready").is_err());
    changed = attestation("ready");
    changed["_meta"]["coucouIsolation"]["inheritedConfigurationDisabled"] = json!(false);
    assert!(acp::validate_manifest(&changed, "ready").is_err());
}

#[test]
fn acp_errors_do_not_expose_provider_secrets_and_updates_are_scoped() {
    let error = acp::result(&json!({"error":{"message":"Authorization: Bearer secret-model-key"}}))
        .unwrap_err();
    assert!(!error.contains("secret-model-key"));
    assert!(acp::result(&json!({"result":[]})).is_err());
    let message = json!({"params":{"sessionId":"owned","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Olá"}}}});
    assert!(acp::scoped_update(&message, "owned").is_some());
    assert!(acp::scoped_update(&message, "foreign").is_none());
    assert!(!acp::valid_session_id("session\nspoof"));
    assert!(!acp::valid_session_id(" "));
    let mut text = "x".repeat(999_999);
    assert!(acp::append(&mut text, "á").is_err());
    assert_eq!(text.len(), 999_999);
}

#[test]
fn model_connection_allows_only_explicit_secure_or_loopback_routes() {
    let mut model = ModelConnection {
        base_url: "http://127.0.0.1:14500/v1".into(),
        model: "groq/test-model".into(),
        api_key: "private-test-key".into(),
        context_length: MIN_CONTEXT_LENGTH,
    };
    assert!(profile::validate_model(&model).is_ok());
    for endpoint in [
        "http://example.com/v1",
        "https://example.com",
        "https://key@example.com/v1",
        "https://example.com/v1?key=secret",
    ] {
        model.base_url = endpoint.into();
        assert!(profile::validate_model(&model).is_err(), "{endpoint}");
    }
    model.base_url = "https://example.com/v1".into();
    model.context_length = 0;
    assert!(profile::validate_model(&model).is_err());
    model.context_length = MIN_CONTEXT_LENGTH - 1;
    assert!(profile::validate_model(&model).is_err());
    model.context_length = MIN_CONTEXT_LENGTH;
    assert!(profile::validate_model(&model).is_ok());
    model.context_length = 1_000_001;
    assert!(profile::validate_model(&model).is_err());
}

#[test]
fn profile_never_adopts_an_existing_unowned_home() {
    let root = std::env::temp_dir().join(format!("coucou-hermes-unowned-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let preserved = root.join("existing-user-context.txt");
    std::fs::write(&preserved, "preserve").unwrap();
    assert!(profile::managed_profile(&root).is_err());
    assert_eq!(std::fs::read_to_string(&preserved).unwrap(), "preserve");
    std::fs::remove_file(preserved).unwrap();
    std::fs::remove_dir(root).unwrap();
}
