//! Configuration is initiated in the trusted settings window, never by a model.
use crate::{
    agent_chat::AgentChat,
    managed_services::{ManagedServiceSettings, ManagedServices, ServiceStatus},
    supervisor::{RuntimeSettings, RuntimeStatus, Supervisor},
    Shared,
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
fn readable(window: &WebviewWindow) -> Result<(), String> {
    if matches!(window.label(), "settings" | "island") {
        Ok(())
    } else {
        Err("Janela não autorizada.".into())
    }
}
fn writable(window: &WebviewWindow, shared: &Shared) -> Result<(), String> {
    readable(window)?;
    if window.label() != "settings"
        || !window.is_focused().unwrap_or(false)
        || shared.paused.load(Ordering::Relaxed)
    {
        return Err("Abra as configurações do Coucou e confirme esta ação.".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn managed_service_status(
    window: WebviewWindow,
    services: State<'_, ManagedServices>,
) -> Result<ServiceStatus, String> {
    readable(&window)?;
    Ok(services.status().await)
}
#[tauri::command]
pub async fn managed_service_configure(
    app: AppHandle,
    window: WebviewWindow,
    shared: State<'_, Shared>,
    services: State<'_, ManagedServices>,
    chat: State<'_, AgentChat>,
    settings: ManagedServiceSettings,
) -> Result<ServiceStatus, String> {
    writable(&window, &shared)?;
    chat.cancel_agent("hermes");
    let result = services.configure(settings).await?;
    let _ = app.emit_to(
        crate::island::WINDOW_LABEL,
        "managed-models-changed",
        &result,
    );
    Ok(result)
}
#[tauri::command]
pub async fn managed_service_set_provider_key(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    services: State<'_, ManagedServices>,
    platform: String,
    value: String,
) -> Result<ServiceStatus, String> {
    writable(&window, &shared)?;
    services.set_provider_key(platform, value).await
}
#[tauri::command]
pub async fn managed_service_provision(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    services: State<'_, ManagedServices>,
) -> Result<ServiceStatus, String> {
    writable(&window, &shared)?;
    services.provision().await
}
#[tauri::command]
pub async fn managed_service_start(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    services: State<'_, ManagedServices>,
) -> Result<ServiceStatus, String> {
    writable(&window, &shared)?;
    services.start().await
}
#[tauri::command]
pub async fn managed_service_stop(
    window: WebviewWindow,
    services: State<'_, ManagedServices>,
    chat: State<'_, AgentChat>,
) -> Result<ServiceStatus, String> {
    readable(&window)?;
    chat.cancel_agent("hermes");
    services.stop().await
}
#[tauri::command]
pub async fn hermes_status(
    window: WebviewWindow,
    supervisor: State<'_, Supervisor>,
    services: State<'_, ManagedServices>,
) -> Result<RuntimeStatus, String> {
    readable(&window)?;
    supervisor.status(&services).await
}
#[tauri::command]
pub async fn hermes_provision(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    supervisor: State<'_, Supervisor>,
    services: State<'_, ManagedServices>,
) -> Result<RuntimeStatus, String> {
    writable(&window, &shared)?;
    supervisor.provision().await?;
    supervisor.status(&services).await
}
#[tauri::command]
pub async fn hermes_configure(
    window: WebviewWindow,
    shared: State<'_, Shared>,
    supervisor: State<'_, Supervisor>,
    services: State<'_, ManagedServices>,
    chat: State<'_, AgentChat>,
    settings: RuntimeSettings,
) -> Result<RuntimeStatus, String> {
    writable(&window, &shared)?;
    Supervisor::validate_settings(&settings)?;
    chat.cancel_agent("hermes");
    supervisor.configure(&settings)?;
    supervisor.status(&services).await
}
