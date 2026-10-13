//! ACP client for the owned Hermes bootstrap, with a live tool attestation.
use super::{EFFECTIVE_TOOL_NAMES, HERMES_REVISION};
use crate::agent_chat::{transport::ProcessIo, RunContext, RunOutcome};
use crate::usage::{activity, Collector};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Duration};

const MAX_TEXT_BYTES: usize = 1_000_000;

pub async fn run(io: &mut ProcessIo, ctx: &RunContext) -> Result<RunOutcome, String> {
    let initialized = rpc(io, "initialize", json!({
        "protocolVersion": 1,
        "clientCapabilities": {"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},
        "clientInfo":{"name":"coucou-managed","version":env!("CARGO_PKG_VERSION")}
    }), "coucou-hermes-initialize").await?;
    if initialized.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
        return Err("O Hermes usa uma versão ACP incompatível.".into());
    }
    validate_manifest(&initialized, "policy")?;
    let mut params = json!({"cwd":io.context().cwd.to_string_lossy(),"mcpServers":[]});
    let (session_id, session) = if let Some(id) = &ctx.session_id {
        if !valid_session_id(id)
            || initialized
                .pointer("/agentCapabilities/loadSession")
                .and_then(Value::as_bool)
                != Some(true)
        {
            return Err("Esta sessão Hermes não pode ser retomada com segurança.".into());
        }
        params["sessionId"] = json!(id);
        (
            id.clone(),
            rpc(io, "session/load", params, "coucou-hermes-load").await?,
        )
    } else {
        let session = rpc(io, "session/new", params, "coucou-hermes-new").await?;
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|id| valid_session_id(id))
            .ok_or("O Hermes não retornou uma sessão válida.")?
            .to_owned();
        (id, session)
    };
    // The source pin and an empty mcpServers request are insufficient. This is
    // the snapshot of the actual AIAgent AFTER registration of the owned bridge.
    validate_manifest(&session, "ready")?;
    io.emit("session", json!({"sessionId":session_id}));
    io.emit(
        "status",
        json!({"text":"Hermes conectado; ferramentas efetivas limitadas à ponte Coucou."}),
    );
    let mut usage = Collector::new("hermes", &session_id, &ctx.run_id);
    usage.version(
        initialized
            .pointer("/agentInfo/version")
            .and_then(Value::as_str),
    );
    if let Some(snapshot) = usage.take_update() {
        io.emit("usage", snapshot);
    }

    let prompt_id = format!("coucou-hermes-prompt-{}", ctx.run_id);
    io.send(
        json!({"jsonrpc":"2.0","id":prompt_id,"method":"session/prompt",
        "params":{"sessionId":session_id,"prompt":[{"type":"text","text":ctx.query}]}}),
    )
    .await?;
    let mut text = String::new();
    loop {
        let message = match io.next().await {
            Ok(message) => message,
            Err(_) if io.is_cancelled() => {
                let _ = io.send(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session_id}})).await;
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    cancellation_grace(io, &prompt_id),
                )
                .await;
                return Ok(RunOutcome {
                    session_id,
                    status: "interrupted".into(),
                    text,
                });
            }
            Err(error) => return Err(error),
        };
        if message.get("id") == Some(&json!(prompt_id)) && message.get("method").is_none() {
            let result = result(&message)?;
            usage.acp_result(&result);
            if let Some(snapshot) = usage.take_update() {
                io.emit("usage", snapshot);
            }
            let status = match result.get("stopReason").and_then(Value::as_str) {
                Some("cancelled") => "interrupted",
                Some("end_turn" | "max_tokens" | "max_turn_requests" | "refusal") => "completed",
                _ => return Err("O Hermes terminou sem um motivo ACP reconhecido.".into()),
            };
            return Ok(RunOutcome {
                session_id,
                status: status.into(),
                text,
            });
        }
        if message.get("id").is_some() && message.get("method").is_some() {
            deny_callback(io, &message).await?;
            return Err("O Hermes tentou acessar um recurso nativo fora da ponte Coucou. O turno foi interrompido.".into());
        }
        if message.get("method").and_then(Value::as_str) != Some("session/update") {
            continue;
        }
        let Some(update) = scoped_update(&message, &session_id) else {
            continue;
        };
        usage.acp_update(&message["params"]);
        if let Some(snapshot) = usage.take_update() {
            io.emit("usage", snapshot);
        }
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk")
                if update.pointer("/content/type").and_then(Value::as_str) == Some("text") =>
            {
                if let Some(delta) = update.pointer("/content/text").and_then(Value::as_str) {
                    append(&mut text, delta)?;
                    io.emit("delta", json!({"text":delta}));
                }
            }
            Some("tool_call" | "tool_call_update") => {
                let id = update
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let status = update
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("pending");
                if let Some(event) = activity(
                    "hermes",
                    &session_id,
                    &ctx.run_id,
                    "tool",
                    id,
                    status,
                    "Hermes ACP/session/update",
                ) {
                    io.emit("activity", event);
                }
                // Raw input/output may contain documents or local credentials.
                let title = update
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Ferramenta do Coucou");
                io.emit("tool", json!({"text":crate::privacy::diagnostic(&title.chars().take(300).collect::<String>()),
                    "status":status.chars().take(40).collect::<String>(),"toolCallId":id.chars().take(256).collect::<String>()}));
            }
            Some("current_mode_update") => {
                return Err(
                    "O Hermes alterou seu modo gerenciado. O turno foi interrompido.".into(),
                )
            }
            _ => {}
        }
    }
}

pub(super) fn validate_manifest(response: &Value, phase: &str) -> Result<(), String> {
    let meta = response
        .pointer("/_meta/coucouIsolation")
        .ok_or("O Hermes não comprovou o isolamento gerenciado.")?;
    if meta.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || meta.get("revision").and_then(Value::as_str) != Some(HERMES_REVISION)
        || meta.get("phase").and_then(Value::as_str) != Some(phase)
        || meta.get("nativeToolsDisabled").and_then(Value::as_bool) != Some(true)
        || meta
            .get("inheritedConfigurationDisabled")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err("O Hermes não corresponde à política de isolamento do Coucou.".into());
    }
    let names = meta
        .get("toolNames")
        .and_then(Value::as_array)
        .ok_or("Manifesto de ferramentas Hermes inválido.")?;
    let actual: Option<BTreeSet<&str>> = names.iter().map(Value::as_str).collect();
    let expected: BTreeSet<&str> = EFFECTIVE_TOOL_NAMES.into_iter().collect();
    if names.len() != expected.len() || actual.as_ref() != Some(&expected) {
        return Err("As ferramentas efetivas do Hermes excedem ou não atendem a ponte Coucou. Nenhum prompt foi enviado.".into());
    }
    Ok(())
}

async fn rpc(io: &mut ProcessIo, method: &str, params: Value, id: &str) -> Result<Value, String> {
    // The private Windows runtime can take minutes to import its pinned SDKs
    // on first launch. The parent still bounds the entire run to ten minutes.
    tokio::time::timeout(Duration::from_secs(300), async {
        io.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        loop {
            let message = io.next().await?;
            if message.get("id") == Some(&json!(id)) && message.get("method").is_none() {
                return result(&message);
            }
            // Consume old-session history without treating replay as new output.
            if message.get("id").is_some() && message.get("method").is_some() {
                deny_callback(io, &message).await?;
                return Err("O Hermes solicitou um recurso nativo durante a inicialização.".into());
            }
        }
    })
    .await
    .map_err(|_| {
        "A inicialização do Hermes excedeu o prazo. Nenhum reenvio automático foi feito.".to_owned()
    })?
}

async fn deny_callback(io: &mut ProcessIo, message: &Value) -> Result<(), String> {
    let response = if message.get("method").and_then(Value::as_str)
        == Some("session/request_permission")
    {
        json!({"jsonrpc":"2.0","id":message["id"],"result":{"outcome":{"outcome":"cancelled"}}})
    } else {
        json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Native client resources are disabled"}})
    };
    io.send(response).await
}

async fn cancellation_grace(io: &mut ProcessIo, prompt_id: &str) {
    while let Ok(message) = io.next_uncancelled(Duration::from_secs(2)).await {
        if message.get("id") == Some(&json!(prompt_id)) && message.get("method").is_none() {
            break;
        }
        if message.get("id").is_some() && message.get("method").is_some() {
            let _ = deny_callback(io, &message).await;
        }
    }
}

pub(super) fn result(message: &Value) -> Result<Value, String> {
    if message.get("error").is_some() {
        // Do not echo server exception text: it can include a URL/key or prompt.
        return Err(
            "O Hermes recusou a operação. Confira runtime, modelo e conexão nas configurações."
                .into(),
        );
    }
    message
        .get("result")
        .filter(|result| result.is_object())
        .cloned()
        .ok_or("Resposta ACP Hermes inválida.".into())
}

pub(super) fn scoped_update<'a>(message: &'a Value, session: &str) -> Option<&'a Value> {
    let params = message.get("params")?;
    if params.get("sessionId").and_then(Value::as_str) != Some(session) {
        return None;
    }
    params.get("update").filter(|value| value.is_object())
}

pub(super) fn valid_session_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}

pub(super) fn append(text: &mut String, delta: &str) -> Result<(), String> {
    if text.len().saturating_add(delta.len()) > MAX_TEXT_BYTES {
        return Err("A resposta Hermes excedeu o limite da conversa.".into());
    }
    text.push_str(delta);
    Ok(())
}
