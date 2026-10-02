//! The webview is the trusted user boundary. Models cannot invoke these commands.
use crate::{documents, island, memory::*, PersonalStore, Shared};
use std::sync::atomic::Ordering;
use tauri::{State, WebviewWindow};

fn readable(window: &WebviewWindow) -> Result<(), String> {
    if matches!(window.label(), "island" | "settings") {
        Ok(())
    } else {
        Err("Esta janela não pode acessar os dados pessoais.".into())
    }
}
fn writable(window: &WebviewWindow, shared: &Shared) -> Result<(), String> {
    readable(window)?;
    if !window.is_focused().unwrap_or(false) || shared.paused.load(Ordering::Relaxed) {
        return Err("Abra o Coucou e confirme esta ação na interface.".into());
    }
    Ok(())
}
fn service(store: &PersonalStore) -> Result<MemoryService, String> {
    store.memory.clone()
}
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| "Não foi possível concluir a operação no armazenamento pessoal.".to_string())?
}

#[tauri::command]
pub async fn memory_preferences(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
) -> Result<MemoryPreferences, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.preferences()).await
}
#[tauri::command]
pub async fn memory_set_preferences(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    preferences: MemoryPreferences,
) -> Result<MemoryPreferences, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.set_preferences(preferences)).await
}
#[tauri::command]
pub async fn memory_list(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    query: MemoryQuery,
) -> Result<Vec<MemoryRecord>, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.list(&query)).await
}
#[tauri::command]
pub async fn memory_propose(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    candidate: MemoryCandidate,
) -> Result<MemoryRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    // Proposals are always pending; claiming source=user never confers authority.
    blocking(move || memory.upsert_candidate(candidate)).await
}
#[tauri::command]
pub async fn memory_approve(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<MemoryRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.approve(&id, revision, true)).await
}
#[tauri::command]
pub async fn memory_reject(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<MemoryRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.reject(&id, revision)).await
}
#[tauri::command]
pub async fn memory_forget(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<usize, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.forget(&id, revision)).await
}
#[tauri::command]
pub async fn memory_export(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    include_history: bool,
) -> Result<String, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.export(include_history)).await
}
#[tauri::command]
pub async fn skills_list(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    query: MemoryQuery,
) -> Result<Vec<SkillRecord>, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.list_skills(&query)).await
}
#[tauri::command]
pub async fn skills_propose(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    candidate: SkillCandidate,
) -> Result<SkillRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.upsert_skill_candidate(candidate)).await
}
#[tauri::command]
pub async fn skills_approve(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<SkillRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.approve_skill(&id, revision, true)).await
}
#[tauri::command]
pub async fn skills_reject(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<SkillRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.reject_skill(&id, revision)).await
}
#[tauri::command]
pub async fn skills_forget(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<usize, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    blocking(move || memory.forget_skill(&id, revision)).await
}
#[tauri::command]
pub async fn skills_diff(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<SkillDiff, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.skill_diff(&id, revision)).await
}
#[tauri::command]
pub async fn skills_load(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<SkillRecord, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.load_skill(&id, revision)).await
}
#[tauri::command]
pub async fn skills_export(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    id: String,
    revision: u64,
) -> Result<SkillExport, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.export_skill(&id, revision)).await
}
#[tauri::command]
pub async fn skills_import(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    markdown: String,
    scope: String,
) -> Result<SkillRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    let source = SourceRef {
        kind: SourceKind::Import,
        reference: "interface:importacao".into(),
        evidence: "Procedimento importado pelo usuário; revisão obrigatória.".into(),
    };
    blocking(move || memory.import_skill(&markdown, &scope, source)).await
}
#[tauri::command]
pub async fn skills_restore(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    id: String,
) -> Result<SkillRecord, String> {
    writable(&window, &shared)?;
    let memory = service(&store)?;
    let source = SourceRef {
        kind: SourceKind::User,
        reference: "interface:restauracao".into(),
        evidence: "Versão anterior escolhida pelo usuário; revisão obrigatória.".into(),
    };
    blocking(move || memory.propose_skill_restore(&id, source)).await
}
#[tauri::command]
pub async fn history_list(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    limit: usize,
) -> Result<Vec<SessionRecord>, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.sessions_list(limit)).await
}
#[tauri::command]
pub async fn history_search(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    query: String,
    limit: usize,
) -> Result<Vec<SessionSearchHit>, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.sessions_search(&query, limit)).await
}
#[tauri::command]
pub async fn history_messages(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    conversation_id: String,
    limit: usize,
) -> Result<Vec<StoredMessage>, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.conversation_messages(&conversation_id, limit)).await
}

#[tauri::command]
pub async fn history_context(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    context_id: String,
    limit: usize,
    budget: usize,
) -> Result<PersonalContextHistory, String> {
    readable(&window)?;
    let memory = service(&store)?;
    blocking(move || memory.personal_context_history(&context_id, limit, budget)).await
}
#[tauri::command]
pub async fn history_forget(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    chat: State<'_, crate::agent_chat::AgentChat>,
    conversation_id: String,
) -> Result<usize, String> {
    writable(&window, &shared)?;
    if chat.has_active_conversation(&conversation_id) {
        return Err("Pare o turno ativo antes de esquecer esta conversa.".into());
    }
    crate::capabilities::broker().revoke(&conversation_id);
    let memory = service(&store)?;
    blocking(move || memory.forget_conversation(&conversation_id)).await
}
#[tauri::command]
pub fn permissions_revoke(window: WebviewWindow, conversation_id: String) -> Result<(), String> {
    readable(&window)?;
    crate::capabilities::broker().revoke(&conversation_id);
    Ok(())
}
fn document_service(store: &PersonalStore) -> Result<documents::DocumentService, String> {
    store.documents.clone()
}
#[tauri::command]
pub async fn documents_choose(
    window: WebviewWindow,
    shared: State<'_, Shared>,
) -> Result<Vec<String>, String> {
    writable(&window, &shared)?;
    let owner = window
        .hwnd()
        .map_err(|_| "Janela indisponível para selecionar documentos.".to_string())?
        .0 as isize;
    blocking(move || crate::file_dialog::choose(owner)).await
}
#[tauri::command]
pub async fn documents_ingest(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    store: State<'_, PersonalStore>,
    conversation_id: String,
    paths: Vec<String>,
) -> Result<Vec<documents::Attachment>, String> {
    if window.label() != island::WINDOW_LABEL || shared.paused.load(Ordering::Relaxed) {
        return Err("Abra o Coucou para anexar documentos.".into());
    }
    let service = document_service(&store)?;
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    service.ingest(&conversation_id, &paths, receiver).await
}
#[tauri::command]
pub async fn documents_list(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    conversation_id: String,
) -> Result<Vec<documents::Attachment>, String> {
    readable(&window)?;
    document_service(&store)?.list(&conversation_id).await
}
#[tauri::command]
pub async fn documents_remove(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    conversation_id: String,
    id: String,
) -> Result<(), String> {
    readable(&window)?;
    document_service(&store)?
        .remove(&conversation_id, &id)
        .await
}
#[tauri::command]
pub async fn documents_prepare(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    conversation_id: String,
    attachment_ids: Vec<String>,
    budget_chars: usize,
) -> Result<documents::PreparedDocuments, String> {
    readable(&window)?;
    let service = document_service(&store)?;
    let (_cancel, receiver) = tokio::sync::watch::channel(false);
    service
        .prepare(&conversation_id, &attachment_ids, budget_chars, receiver)
        .await
}
#[tauri::command]
pub async fn documents_read(
    window: WebviewWindow,
    store: State<'_, PersonalStore>,
    conversation_id: String,
    id: String,
    offset_chars: usize,
    limit_chars: usize,
) -> Result<documents::DocumentChunk, String> {
    readable(&window)?;
    document_service(&store)?
        .read_text(&conversation_id, &id, offset_chars, limit_chars)
        .await
}
