//! Owns local chat sessions. Only explicit UI actions start turns or grant decisions.
mod acp;
mod claude_cli;
mod codex;
#[cfg(test)]
mod personal_smoke;
mod process;
pub mod transport;
#[cfg(test)]
mod transport_tests;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, watch};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRequest {
    pub conversation_id: String,
    pub run_id: String,
    pub agent: String,
    pub cwd: String,
    pub session_id: Option<String>,
    pub query: String,
    pub writable: bool,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    #[serde(default)]
    pub context_id: Option<String>,
}
#[derive(Clone)]
pub struct RunContext {
    pub conversation_id: String,
    pub run_id: String,
    pub agent: String,
    pub cwd: PathBuf,
    pub session_id: Option<String>,
    pub query: String,
    pub writable: bool,
    pub personal: bool,
    pub context_id: Option<String>,
}
pub struct RunOutcome {
    pub session_id: String,
    pub status: String,
    pub text: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    agent: String,
    available: bool,
    path: Option<String>,
    detail: String,
    readonly_only: bool,
    personal_supported: bool,
}
const UNQUALIFIED_ISOLATION: &str = "O chat deste fornecedor está bloqueado: o isolamento de hooks e MCP da CLI ainda não foi qualificado. O monitoramento de atividade continua disponível.";

fn validate_provider_isolation(request: &StartRequest) -> Result<(), String> {
    match request.agent.as_str() {
        "codex" | "claude" => Ok(()),
        "gemini" | "copilot" => Err(UNQUALIFIED_ISOLATION.into()),
        _ => Err("Unsupported chat agent.".into()),
    }
}

fn resolved_status(agent: &str, path: String) -> CliStatus {
    let qualified = matches!(agent, "codex" | "claude");
    CliStatus {
        agent: agent.into(),
        available: qualified,
        path: Some(path),
        readonly_only: agent == "claude",
        personal_supported: qualified,
        detail: if !qualified {
            format!("CLI detectada. {UNQUALIFIED_ISOLATION}")
        } else if agent == "claude" {
            "Chat pessoal sem ferramentas nativas; no modo projeto, somente leitura.".into()
        } else {
            "Conversa pessoal com contexto compartilhado. Acesso a arquivos exige permissão por operação.".into()
        },
    }
}

pub fn status() -> Vec<CliStatus> {
    ["codex", "claude", "gemini", "copilot"]
        .iter()
        .map(|agent| match process::resolve(agent) {
            Ok(exe) => resolved_status(agent, exe.display.to_string_lossy().into()),
            Err(error) => CliStatus {
                agent: (*agent).into(),
                available: false,
                path: None,
                readonly_only: *agent == "claude",
                personal_supported: matches!(*agent, "codex" | "claude"),
                detail: error,
            },
        })
        .collect()
}

struct Decision {
    conversation_id: String,
    run_id: String,
    deadline: Instant,
    sender: oneshot::Sender<String>,
    conversation_scope: bool,
}
#[derive(Default)]
pub struct Control {
    decisions: Mutex<HashMap<String, Decision>>,
    counter: AtomicU64,
}
impl Control {
    fn insert(&self, ctx: &RunContext) -> Result<(String, oneshot::Receiver<String>), String> {
        self.insert_scoped(ctx, false)
    }
    fn insert_scoped(
        &self,
        ctx: &RunContext,
        conversation_scope: bool,
    ) -> Result<(String, oneshot::Receiver<String>), String> {
        let mut decisions = self
            .decisions
            .lock()
            .map_err(|_| "Approval state unavailable.")?;
        decisions.retain(|_, decision| {
            decision.deadline > Instant::now() && !decision.sender.is_closed()
        });
        if decisions.values().any(|decision| {
            decision.conversation_id == ctx.conversation_id && decision.run_id == ctx.run_id
        }) {
            return Err("This turn already has a pending approval.".into());
        }
        if decisions.len() >= 8 {
            return Err("Too many pending chat approvals.".into());
        }
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            ctx.run_id,
            self.counter.fetch_add(1, Ordering::Relaxed)
        );
        let (sender, receiver) = oneshot::channel();
        decisions.insert(
            id.clone(),
            Decision {
                conversation_id: ctx.conversation_id.clone(),
                run_id: ctx.run_id.clone(),
                deadline: Instant::now() + Duration::from_secs(120),
                sender,
                conversation_scope,
            },
        );
        Ok((id, receiver))
    }
    fn remove(&self, id: &str) {
        if let Ok(mut decisions) = self.decisions.lock() {
            decisions.remove(id);
        }
    }
    fn decide(&self, conversation: &str, run: &str, id: &str, choice: &str) -> bool {
        if !matches!(choice, "allow" | "allowConversation" | "deny") {
            return false;
        }
        let Ok(mut decisions) = self.decisions.lock() else {
            return false;
        };
        let Some(decision) = decisions.get(id) else {
            return false;
        };
        if choice == "allowConversation" && !decision.conversation_scope {
            return false;
        }
        if decision.conversation_id != conversation
            || decision.run_id != run
            || decision.deadline <= Instant::now()
        {
            return false;
        }
        decisions
            .remove(id)
            .is_some_and(|decision| decision.sender.send(choice.into()).is_ok())
    }
    fn cancel(&self, conversation: &str, run: &str) {
        if let Ok(mut decisions) = self.decisions.lock() {
            decisions.retain(|_, decision| {
                decision.conversation_id != conversation || decision.run_id != run
            });
        }
    }
}
struct Entry {
    agent: String,
    cwd: PathBuf,
    writable: bool,
    session_id: Option<String>,
    run_id: String,
    cancel: Option<watch::Sender<bool>>,
}
#[derive(Default)]
struct Inner {
    entries: Mutex<HashMap<String, Entry>>,
    control: Arc<Control>,
}
#[derive(Clone, Default)]
pub struct AgentChat {
    inner: Arc<Inner>,
}

fn validate_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}
fn context(request: StartRequest) -> Result<RunContext, String> {
    validate_provider_isolation(&request)?;
    if !validate_id(&request.conversation_id)
        || !validate_id(&request.run_id)
        || request
            .session_id
            .as_ref()
            .is_some_and(|id| !validate_id(id))
    {
        return Err("Invalid conversation or run ID.".into());
    }
    if request.query.trim().is_empty() || request.query.len() > 65536 {
        return Err("Enter a message up to 64 KiB.".into());
    }
    let personal = request.cwd.trim().is_empty();
    if request
        .context_id
        .as_deref()
        .is_some_and(|id| id != "personal-main")
        || (!personal && request.context_id.is_some())
    {
        return Err("O contexto pessoal só pode ser usado no chat pessoal do Coucou.".into());
    }
    if personal && request.writable {
        return Err("O modo pessoal não autoriza alterações no computador.".into());
    }
    if personal && !matches!(request.agent.as_str(), "codex" | "claude") {
        return Err("O modo pessoal deste agente ainda não tem acesso restrito qualificado. Use o modo projeto.".into());
    }
    if personal
        && !request
            .conversation_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Identificador de conversa inválido para o workspace pessoal.".into());
    }
    let cwd = if personal {
        let path = crate::settings::local_dir()
            .join("conversations")
            .join(&request.conversation_id)
            .join("workspace");
        std::fs::create_dir_all(&path)
            .map_err(|_| "Não foi possível criar o workspace pessoal.")?;
        path
    } else {
        PathBuf::from(&request.cwd)
    };
    if request.cwd.len() > 4096 || !cwd.is_absolute() {
        return Err("Select an absolute project directory before sending.".into());
    }
    let cwd = cwd
        .canonicalize()
        .map_err(|_| "The project directory does not exist or cannot be accessed.")?;
    if !cwd.is_dir() {
        return Err("The project path must be a directory.".into());
    }
    let cwd = process::cli_path(&cwd);
    if request.agent == "claude" && request.writable {
        return Err(
            "Claude CLI chat is currently read-only. Use Codex for project changes.".into(),
        );
    }
    Ok(RunContext {
        conversation_id: request.conversation_id,
        run_id: request.run_id,
        agent: request.agent,
        cwd,
        session_id: request.session_id,
        query: request.query,
        writable: request.writable,
        personal,
        context_id: if personal {
            Some("personal-main".into())
        } else {
            None
        },
    })
}

impl AgentChat {
    pub fn start(&self, app: AppHandle, request: StartRequest) -> Result<(), String> {
        // Fail before workspace creation, provenance/history, executable lookup
        // or spawning. ACP implementations stay dormant until isolation is qualified.
        validate_provider_isolation(&request)?;
        let attachment_ids = request.attachment_ids.clone();
        if attachment_ids.len() > 10 {
            return Err("Anexe no máximo 10 documentos por turno.".into());
        }
        let mut ctx = context(request)?;
        let executable = process::resolve(&ctx.agent)?;
        let (sender, receiver) = watch::channel(false);
        {
            let mut entries = self
                .inner
                .entries
                .lock()
                .map_err(|_| "Chat state unavailable.")?;
            if entries
                .values()
                .filter(|entry| entry.cancel.is_some())
                .count()
                >= 4
            {
                return Err("Four CLI turns are already active. Stop or finish one first.".into());
            }
            if let Some(entry) = entries.get(&ctx.conversation_id) {
                if entry.cancel.is_some() {
                    return Err("This conversation already has an active turn.".into());
                }
                if entry.agent != ctx.agent
                    || entry.cwd != ctx.cwd
                    || entry.writable != ctx.writable
                {
                    return Err(
                        "Create a new conversation when changing agent, project or permissions."
                            .into(),
                    );
                }
                if entry.run_id == ctx.run_id {
                    return Err("This run ID was already used. Create a new turn.".into());
                }
                if ctx.session_id.is_some() && ctx.session_id != entry.session_id {
                    return Err(
                        "Create a new conversation to resume a different native session.".into(),
                    );
                }
                if ctx.session_id.is_none() {
                    ctx.session_id = entry.session_id.clone();
                }
            }
            if let Some(session) = &ctx.session_id {
                if entries.values().any(|entry| {
                    entry.agent == ctx.agent
                        && entry.session_id.as_ref() == Some(session)
                        && entry.cancel.is_some()
                }) {
                    return Err(
                        "This native session is already controlled by another Coucou conversation."
                            .into(),
                    );
                }
            }
            if entries.len() >= 32 && !entries.contains_key(&ctx.conversation_id) {
                let removable = entries
                    .iter()
                    .find(|(_, entry)| entry.cancel.is_none())
                    .map(|(id, _)| id.clone())
                    .ok_or("Chat conversation limit reached.")?;
                entries.remove(&removable);
            }
            entries.insert(
                ctx.conversation_id.clone(),
                Entry {
                    agent: ctx.agent.clone(),
                    cwd: ctx.cwd.clone(),
                    writable: ctx.writable,
                    session_id: ctx.session_id.clone(),
                    run_id: ctx.run_id.clone(),
                    cancel: Some(sender),
                },
            );
        }
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let scoped = ctx.clone();
            let original_message = ctx.query.clone();
            let event_app = app.clone();
            let event_manager = manager.clone();
            let sink: transport::Sink =
                Arc::new(move |kind, data| event_manager.emit(&event_app, &scoped, kind, data));
            let memory = app.state::<crate::PersonalStore>().memory.clone();
            let prepared=async {
                let store=memory.clone()?;
                let provenance=ctx.clone();
                let input=format!("{}\nAnexos: {}",ctx.query,serde_json::to_string(&attachment_ids).map_err(|_|"Anexos inválidos.")?);
                let external=tokio::task::spawn_blocking(move||{
                    let external=match &provenance.session_id {
                        Some(id)=>!store.native_session_owned(&provenance.conversation_id,&provenance.agent,id)?,None=>false,
                    };
                    store.reserve_dispatch(&provenance.conversation_id,&provenance.run_id,&provenance.agent,provenance.session_id.as_deref(),&input,external)?;
                    Ok::<_,String>(external)
                }).await.map_err(|_|"Não foi possível verificar a origem desta sessão.")??;
                if external && ctx.personal {return Err("Para preservar o contexto de uma sessão externa, retome-a no modo projeto. O Coucou pedirá autorização antes de cada mensagem.".into());}
                crate::personal_runtime::prepare(&app,&mut ctx,&attachment_ids,receiver.clone(),|kind,data|sink(kind,data)).await?;
                if external {
                    approve_external_session(&manager.inner.control,&ctx,&sink,receiver.clone()).await?;
                }
                if *receiver.borrow(){return Err("Envio cancelado antes de iniciar o agente.".into());}
                let store=memory.clone()?;let run=ctx.run_id.clone();
                tokio::task::spawn_blocking(move||store.dispatch_status(&run,"sending")).await.map_err(|_|"Não foi possível registrar o envio.")??;
                Ok::<_,String>(())
            }.await;
            let args = match prepared {
                Err(error) => Err(error),
                Ok(()) => match ctx.agent.as_str() {
                    "codex" => Ok(vec![
                        "app-server".into(),
                        "--listen".into(),
                        "stdio://".into(),
                    ]),
                    "copilot" => Ok(acp::args(&ctx, "")),
                    "gemini" => {
                        let mut probe_cancel = receiver.clone();
                        process::gemini_flag(&executable, &ctx.cwd, &mut probe_cancel)
                            .await
                            .map(|flag| acp::args(&ctx, &flag))
                    }
                    "claude" => claude_cli::args(&ctx),
                    _ => Err("Unknown CLI.".into()),
                },
            };
            let result = match args {
                Ok(args) => match transport::ProcessIo::spawn(
                    &executable,
                    &args,
                    &ctx,
                    receiver,
                    manager.inner.control.clone(),
                    sink,
                ) {
                    Ok(mut io) => {
                        io.set_user_message(original_message);
                        io.set_memory_service(
                            app.state::<crate::PersonalStore>().memory.clone().ok(),
                        );
                        io.set_document_service(
                            app.state::<crate::PersonalStore>().documents.clone().ok(),
                            attachment_ids,
                        );
                        let result = match ctx.agent.as_str() {
                            "codex" => codex::run(&mut io, &ctx).await,
                            "claude" => claude_cli::run(&mut io, &ctx).await,
                            _ => acp::run(&mut io, &ctx).await,
                        };
                        io.shutdown().await;
                        result
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            };
            let cancelled = manager.is_cancelled(&ctx.conversation_id, &ctx.run_id);
            if let Ok(store) = memory {
                let created_session = if ctx.session_id.is_none() {
                    manager.inner.entries.lock().ok().and_then(|entries| {
                        entries
                            .get(&ctx.conversation_id)
                            .and_then(|entry| entry.session_id.clone())
                    })
                } else {
                    None
                };
                let conversation = ctx.conversation_id.clone();
                let agent = ctx.agent.clone();
                let run = ctx.run_id.clone();
                let receipt_status = if cancelled {
                    "interrupted"
                } else if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                };
                if let Err(error) = tokio::task::spawn_blocking(move || {
                    if let Some(session) = created_session {
                        store.register_native_session(&conversation, &agent, &session)?;
                    }
                    store.dispatch_status(&run, receipt_status)
                })
                .await
                .unwrap_or_else(|_| Err("Registro de envio indisponível.".into()))
                {
                    manager.emit(&app, &ctx, "status", json!({"text":error}));
                }
            }
            let completion = match result {
                Ok(outcome) => {
                    if let Err(error) = crate::personal_runtime::store_reply(
                        &app,
                        &ctx,
                        &outcome.session_id,
                        &outcome.text,
                    )
                    .await
                    {
                        manager.emit(
                            &app,
                            &ctx,
                            "history",
                            json!({"persisted":false,"reason":error}),
                        );
                    }
                    if !outcome.text.is_empty() {
                        manager.emit(&app, &ctx, "message", json!({"text":outcome.text}));
                    }
                    if !outcome.session_id.is_empty() {
                        manager.emit(
                            &app,
                            &ctx,
                            "session",
                            json!({"sessionId":outcome.session_id}),
                        );
                    }
                    json!({"status":if cancelled{"interrupted"}else{&outcome.status}})
                }
                Err(error) => {
                    json!({"status":if cancelled{"interrupted"}else{"failed"},"error":crate::privacy::diagnostic(&error)})
                }
            };
            manager
                .inner
                .control
                .cancel(&ctx.conversation_id, &ctx.run_id);
            if let Ok(mut entries) = manager.inner.entries.lock() {
                if let Some(entry) = entries.get_mut(&ctx.conversation_id) {
                    if entry.run_id == ctx.run_id {
                        entry.cancel = None;
                    }
                }
            }
            // Release the active turn before announcing completion so an immediate
            // next UI turn cannot race the backend cleanup.
            let _=app.emit_to(crate::island::WINDOW_LABEL,"agent-chat",json!({"conversationId":ctx.conversation_id,"runId":ctx.run_id,"kind":"completed","data":completion}));
        });
        Ok(())
    }
    fn emit(&self, app: &AppHandle, ctx: &RunContext, kind: &str, mut data: Value) {
        if matches!(kind, "tool" | "status" | "error") {
            for key in ["text", "message"] {
                if let Some(text) = data.get(key).and_then(Value::as_str) {
                    data[key] = json!(crate::privacy::diagnostic(text));
                }
            }
        }
        if let Ok(mut entries) = self.inner.entries.lock() {
            let valid = entries
                .get(&ctx.conversation_id)
                .is_some_and(|entry| entry.run_id == ctx.run_id && entry.cancel.is_some());
            if !valid {
                return;
            }
            if kind == "session" {
                if let Some(session) = data.get("sessionId").and_then(Value::as_str) {
                    if !validate_id(session) {
                        return;
                    }
                    let conflict = entries.iter().any(|(id, entry)| {
                        id != &ctx.conversation_id
                            && entry.agent == ctx.agent
                            && entry.session_id.as_deref() == Some(session)
                            && entry.cancel.is_some()
                    });
                    if let Some(entry) = entries.get_mut(&ctx.conversation_id) {
                        if conflict {
                            if let Some(cancel) = &entry.cancel {
                                let _ = cancel.send(true);
                            }
                            return;
                        }
                        entry.session_id = Some(session.into());
                    }
                }
            }
        }
        let _=app.emit_to(crate::island::WINDOW_LABEL,"agent-chat",json!({"conversationId":ctx.conversation_id,"runId":ctx.run_id,"kind":kind,"data":data}));
    }
    fn is_cancelled(&self, conversation: &str, run: &str) -> bool {
        self.inner
            .entries
            .lock()
            .ok()
            .and_then(|entries| {
                entries
                    .get(conversation)
                    .filter(|entry| entry.run_id == run)
                    .and_then(|entry| entry.cancel.as_ref().map(|sender| *sender.borrow()))
            })
            .unwrap_or(true)
    }
    pub fn cancel(&self, conversation: &str, run: &str) -> bool {
        let sent = self
            .inner
            .entries
            .lock()
            .ok()
            .and_then(|entries| {
                entries
                    .get(conversation)
                    .filter(|entry| entry.run_id == run)
                    .and_then(|entry| {
                        entry
                            .cancel
                            .as_ref()
                            .map(|sender| sender.send(true).is_ok())
                    })
            })
            .unwrap_or(false);
        self.inner.control.cancel(conversation, run);
        sent
    }
    pub fn cancel_all(&self) {
        if let Ok(entries) = self.inner.entries.lock() {
            for (id, entry) in entries.iter() {
                if let Some(sender) = &entry.cancel {
                    let _ = sender.send(true);
                    self.inner.control.cancel(id, &entry.run_id);
                }
            }
        }
    }
    pub fn decide(&self, conversation: &str, run: &str, id: &str, choice: &str) -> bool {
        if self.is_cancelled(conversation, run) {
            return false;
        }
        self.inner.control.decide(conversation, run, id, choice)
    }
    pub fn owns_session(&self, agent: &str, session: &str) -> bool {
        self.inner.entries.lock().is_ok_and(|entries| {
            entries.values().any(|entry| {
                entry.agent == agent
                    && entry.session_id.as_deref() == Some(session)
                    && entry.cancel.is_some()
            })
        })
    }
    pub fn has_active_conversation(&self, conversation: &str) -> bool {
        self.inner
            .entries
            .lock()
            .map(|entries| {
                entries
                    .get(conversation)
                    .is_some_and(|entry| entry.cancel.is_some())
            })
            .unwrap_or(true)
    }
}

async fn approve_external_session(
    control: &Control,
    ctx: &RunContext,
    sink: &transport::Sink,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let session = ctx
        .session_id
        .as_deref()
        .ok_or("Sessão externa sem identificador.")?;
    let detail=format!("Destino: {} / sessão {}\nEsta sessão não foi criada nesta conversa pelo Coucou. A mensagem poderá alterar a direção do trabalho existente. Feche o terminal que controla essa sessão antes de continuar.\n\nMensagem integral que será enviada:\n{}\n\nEsta autorização permite somente este envio. Não libera mensagens futuras, tarefas agendadas ou outros chats.",ctx.agent,session,ctx.query);
    if detail.chars().count() > 16_384 {
        return Err("A prévia para a sessão externa excede 16.384 caracteres. Reduza a mensagem ou inicie uma nova conversa no Coucou.".into());
    }
    let (request, answer) = control.insert(ctx)?;
    sink(
        "approval",
        json!({"requestId":request,"title":"Autorizar mensagem em chat externo?","detail":detail,"choices":["allow","deny"]}),
    );
    let choice = tokio::select! {biased;
        _=cancel.changed()=>"deny".to_string(),
        value=tokio::time::timeout(Duration::from_secs(120),answer)=>value.ok().and_then(Result::ok).unwrap_or_else(||"deny".into()),
    };
    control.remove(&request);
    sink("approvalResolved", json!({"requestId":request}));
    if choice != "allow" || *cancel.borrow() {
        return Err("O envio ao chat externo foi negado, cancelado ou expirou. Nenhuma mensagem foi enviada ao agente.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn isolation_request(
        agent: &str,
        personal: bool,
        writable: bool,
        resumed: bool,
    ) -> StartRequest {
        StartRequest {
            conversation_id: format!("unqualified-provider-{}", std::process::id()),
            run_id: "isolated-run".into(),
            agent: agent.into(),
            cwd: if personal {
                String::new()
            } else {
                std::env::temp_dir()
                    .join(format!(
                        "coucou-unqualified-workspace-{}",
                        std::process::id()
                    ))
                    .to_string_lossy()
                    .into()
            },
            session_id: resumed.then(|| "existing-session".into()),
            query: "Conte os arquivos desta pasta.".into(),
            writable,
            attachment_ids: Vec::new(),
            context_id: personal.then(|| "personal-main".into()),
        }
    }

    #[test]
    fn unqualified_providers_cannot_bypass_isolation_with_mode_or_resume() {
        for agent in ["codex", "claude", "gemini", "copilot"] {
            for personal in [false, true] {
                for writable in [false, true] {
                    for resumed in [false, true] {
                        let request = isolation_request(agent, personal, writable, resumed);
                        let result = validate_provider_isolation(&request);
                        if matches!(agent, "gemini" | "copilot") {
                            assert_eq!(result, Err(UNQUALIFIED_ISOLATION.into()));
                        } else {
                            assert!(result.is_ok());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn unqualified_contexts_fail_before_creating_or_resolving_workspaces() {
        for agent in ["gemini", "copilot"] {
            for personal in [false, true] {
                let request = isolation_request(agent, personal, false, false);
                let workspace = if personal {
                    crate::settings::local_dir()
                        .join("conversations")
                        .join(&request.conversation_id)
                        .join("workspace")
                } else {
                    PathBuf::from(&request.cwd)
                };
                assert!(
                    !workspace.exists(),
                    "The isolated test workspace must not exist"
                );
                let error = match context(request) {
                    Ok(_) => panic!("An unqualified provider must be rejected"),
                    Err(error) => error,
                };
                assert_eq!(error, UNQUALIFIED_ISOLATION);
                assert!(
                    !workspace.exists(),
                    "Provider gating must not create a workspace"
                );
            }
        }
    }

    #[test]
    fn detected_unqualified_clis_remain_visible_with_chat_blocked() {
        for agent in ["gemini", "copilot"] {
            let detected = resolved_status(agent, "C:\\fixture\\agent.cmd".into());
            assert!(!detected.available);
            assert!(!detected.personal_supported);
            assert_eq!(detected.path.as_deref(), Some("C:\\fixture\\agent.cmd"));
            assert!(detected.detail.contains("CLI detectada"));
            assert!(detected.detail.contains(UNQUALIFIED_ISOLATION));
        }
    }

    fn ctx() -> RunContext {
        RunContext {
            conversation_id: "c".into(),
            run_id: "r".into(),
            agent: "codex".into(),
            cwd: PathBuf::from("D:/"),
            session_id: None,
            query: "Hello".into(),
            writable: false,
            personal: false,
            context_id: None,
        }
    }
    #[test]
    fn decisions_are_scoped_and_consumed_once() {
        let control = Control::default();
        let (id, mut receiver) = control.insert(&ctx()).unwrap();
        assert!(!control.decide("other", "r", &id, "allow"));
        assert!(!control.decide("c", "old", &id, "allow"));
        assert!(!control.decide("c", "r", &id, "acceptForSession"));
        assert!(control.decide("c", "r", &id, "allow"));
        assert!(!control.decide("c", "r", &id, "allow"));
        assert_eq!(receiver.try_recv().unwrap(), "allow");
    }
    #[test]
    fn cancel_and_expiry_never_grant() {
        let control = Control::default();
        let (id, mut receiver) = control.insert(&ctx()).unwrap();
        control.cancel("c", "r");
        assert!(receiver.try_recv().is_err());
        assert!(!control.decide("c", "r", &id, "allow"));
        let (id, _receiver) = control.insert(&ctx()).unwrap();
        control
            .decisions
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .deadline = Instant::now();
        assert!(!control.decide("c", "r", &id, "allow"));
    }
    #[tokio::test]
    async fn external_message_authorization_is_exact_and_never_becomes_permanent() {
        let control = Arc::new(Control::default());
        let mut context = ctx();
        context.session_id = Some("existing-thread".into());
        let (events_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let sink: transport::Sink = Arc::new(move |kind, data| {
            let _ = events_tx.send((kind.to_owned(), data));
        });
        let (_cancel, receiver) = watch::channel(false);
        let task_control = control.clone();
        let task_context = context.clone();
        let task_sink = sink.clone();
        let pending = tokio::spawn(async move {
            approve_external_session(&task_control, &task_context, &task_sink, receiver).await
        });
        let (kind, event) = events.recv().await.unwrap();
        assert_eq!(kind, "approval");
        assert!(event["detail"]
            .as_str()
            .unwrap()
            .contains("existing-thread"));
        assert!(event["detail"].as_str().unwrap().contains(&context.query));
        let id = event["requestId"].as_str().unwrap();
        assert!(!control.decide("c", "r", id, "allowConversation"));
        assert!(!control.decide("another", "r", id, "allow"));
        assert!(control.decide("c", "r", id, "allow"));
        assert!(!control.decide("c", "r", id, "allow"));
        assert!(pending.await.unwrap().is_ok());
        context.run_id = "next".into();
        let (_cancel, receiver) = watch::channel(false);
        let next_control = control.clone();
        let pending = tokio::spawn(async move {
            approve_external_session(&next_control, &context, &sink, receiver).await
        });
        let mut next = events.recv().await.unwrap();
        if next.0 == "approvalResolved" {
            next = events.recv().await.unwrap();
        }
        assert_eq!(next.0, "approval");
        assert!(control.decide("c", "next", next.1["requestId"].as_str().unwrap(), "deny"));
        assert!(pending.await.unwrap().is_err());
    }
}
