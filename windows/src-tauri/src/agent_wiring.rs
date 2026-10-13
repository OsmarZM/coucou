//! Integration boundary: official UI/hooks stay owned by the upstream app.
use crate::{agent_chat, agent_runtime, documents, island, managed_commands, managed_services, memory, personal_commands, supervisor, PersonalStore, Shared};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
/// Metadata only. Execution stays behind the active Codex turn and approval broker.
#[tauri::command]
fn agent_runtime_tools() -> serde_json::Value {
    agent_runtime::specs()
}

#[tauri::command]
async fn agent_chat_status(
    supervisor: State<'_, supervisor::Supervisor>,
    services: State<'_, managed_services::ManagedServices>,
) -> Result<Vec<agent_chat::CliStatus>, String> {
    let mut statuses = agent_chat::status();
    let status = supervisor.status(&services).await?;
    statuses.push(agent_chat::hermes_status(status.available, status.detail));
    Ok(statuses)
}

#[tauri::command]
fn agent_chat_start(
    app: AppHandle,
    window: tauri::WebviewWindow,
    shared: State<Shared>,
    chat: State<agent_chat::AgentChat>,
    request: agent_chat::StartRequest,
) -> Result<(), String> {
    if window.label() != island::WINDOW_LABEL || shared.paused.load(Ordering::Relaxed) {
        return Err("CLI chat is unavailable while Coucou is paused.".into());
    }
    chat.start(app, request)
}

#[tauri::command]
fn agent_chat_cancel(
    window: tauri::WebviewWindow,
    chat: State<agent_chat::AgentChat>,
    conversation_id: String,
    run_id: String,
) -> bool {
    window.label() == island::WINDOW_LABEL && chat.cancel(&conversation_id, &run_id)
}

#[tauri::command]
fn agent_chat_decide(
    window: tauri::WebviewWindow,
    shared: State<Shared>,
    chat: State<agent_chat::AgentChat>,
    conversation_id: String,
    run_id: String,
    request_id: String,
    decision: String,
) -> bool {
    if window.label() != island::WINDOW_LABEL {
        return false;
    }
    if matches!(decision.as_str(), "allow" | "allowConversation")
        && (!window.is_focused().unwrap_or(false) || shared.paused.load(Ordering::Relaxed))
    {
        chat.decide(&conversation_id, &run_id, &request_id, "deny");
        return false;
    }
    chat.decide(&conversation_id, &run_id, &request_id, &decision)
}


pub fn handles(name: &str)->bool { matches!(name, "agent_runtime_tools" | "agent_chat_status" | "agent_chat_start" | "agent_chat_cancel" | "agent_chat_decide" | "memory_preferences" | "memory_set_preferences" | "memory_list" | "memory_propose" | "memory_approve" | "memory_reject" | "memory_forget" | "memory_export" | "skills_list" | "skills_propose" | "skills_approve" | "skills_reject" | "skills_forget" | "skills_diff" | "skills_load" | "skills_export" | "skills_import" | "skills_restore" | "history_list" | "history_search" | "history_context" | "history_messages" | "history_forget" | "permissions_revoke" | "documents_ingest" | "documents_cancel" | "documents_choose" | "documents_list" | "documents_remove" | "documents_prepare" | "documents_read" | "managed_service_status" | "managed_service_configure" | "managed_service_set_provider_key" | "managed_service_provision" | "managed_service_start" | "managed_service_stop" | "hermes_status" | "hermes_provision" | "hermes_configure") }
pub fn invoke(invoke: tauri::ipc::Invoke<tauri::Wry>)->bool { let handler: fn(tauri::ipc::Invoke<tauri::Wry>)->bool = tauri::generate_handler![agent_runtime_tools,agent_chat_status,agent_chat_start,agent_chat_cancel,agent_chat_decide,personal_commands::memory_preferences,personal_commands::memory_set_preferences,personal_commands::memory_list,personal_commands::memory_propose,personal_commands::memory_approve,personal_commands::memory_reject,personal_commands::memory_forget,personal_commands::memory_export,personal_commands::skills_list,personal_commands::skills_propose,personal_commands::skills_approve,personal_commands::skills_reject,personal_commands::skills_forget,personal_commands::skills_diff,personal_commands::skills_load,personal_commands::skills_export,personal_commands::skills_import,personal_commands::skills_restore,personal_commands::history_list,personal_commands::history_search,personal_commands::history_context,personal_commands::history_messages,personal_commands::history_forget,personal_commands::permissions_revoke,personal_commands::documents_ingest,personal_commands::documents_cancel,personal_commands::documents_choose,personal_commands::documents_list,personal_commands::documents_remove,personal_commands::documents_prepare,personal_commands::documents_read,managed_commands::managed_service_status,managed_commands::managed_service_configure,managed_commands::managed_service_set_provider_key,managed_commands::managed_service_provision,managed_commands::managed_service_start,managed_commands::managed_service_stop,managed_commands::hermes_status,managed_commands::hermes_provision,managed_commands::hermes_configure]; handler(invoke) }
pub fn setup(app:&AppHandle) {
 app.manage(agent_chat::AgentChat::default());
 app.manage(documents::ImportOperations::default());
 app.manage(managed_services::ManagedServices::new(crate::settings::local_dir().join("managed-services/free-models")));
 app.manage(supervisor::Supervisor::new(crate::settings::local_dir().join("managed-services/hermes")));
 app.manage(PersonalStore {
 memory: memory::MemoryService::open(crate::settings::local_dir().join("personal/state.db")).and_then(|service| { service.enable_automatic_personal_context()?; Ok(service) }),
 documents: documents::DocumentService::new(crate::settings::local_dir().join("documents")),
 });
 let owned=app.clone();
 tauri::async_runtime::spawn(async move { let services=owned.state::<managed_services::ManagedServices>(); if services.status().await.settings.enabled { let _=services.start().await; } });
}
pub fn cancel(app:&AppHandle) { app.state::<documents::ImportOperations>().cancel_all(); app.state::<agent_chat::AgentChat>().cancel_all(); }
