// Local bounded transport. Failures return to the CLI without a decision.
pub use crate::approvals::Pending;
use crate::approvals::Reply;
use crate::{island::WINDOW_LABEL, log, Shared};
use coucou_agent_protocol::{normalize, Agent, AgentEvent, AgentEventType};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{mpsc, Semaphore};

const MAX_PAYLOAD: usize = 64 * 1024;
const ACK_TIMEOUT: Duration = Duration::from_millis(800);

pub fn pipe_name() -> String {
    let key = crate::win_user::current_user_sid()
        .unwrap_or_else(|| std::env::var("USERNAME").unwrap_or_else(|_| "user".into()));
    format!(r"\\.\pipe\coucou-{key}")
}

fn create_pipe(name: &str, first: bool) -> std::io::Result<NamedPipeServer> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    let sid = crate::win_user::current_user_sid()
        .ok_or_else(|| std::io::Error::other("Cannot determine pipe user"))?;
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(std::io::Error::other)?;
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let result = ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            );
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        result
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let name = pipe_name();
        let mut server = match create_pipe(&name, true) {
            Ok(server) => server,
            Err(err) => {
                log::line(format!("cannot open relay pipe: {err}"));
                return;
            }
        };
        let slots = Arc::new(Semaphore::new(64));
        loop {
            if server.connect().await.is_err() {
                continue;
            }
            let next = match create_pipe(&name, false) {
                Ok(server) => server,
                Err(err) => {
                    log::line(format!("cannot reopen relay pipe: {err}"));
                    return;
                }
            };
            let connected = std::mem::replace(&mut server, next);
            let Ok(permit) = slots.clone().try_acquire_owned() else {
                continue;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                handle(app, connected).await;
            });
        }
    });
}

async fn read_line(reader: &mut (impl AsyncRead + Unpin)) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let count = reader.read(&mut chunk).await.ok()?;
        if count == 0 {
            return (!bytes.is_empty()).then_some(bytes);
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_PAYLOAD {
            return None;
        }
        if let Some(end) = bytes.iter().position(|byte| *byte == b'\n') {
            bytes.truncate(end);
            return Some(bytes);
        }
    }
}

fn decode(bytes: &[u8]) -> Option<AgentEvent> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value.get("protocolVersion").is_some() {
        let event: AgentEvent = serde_json::from_value(value).ok()?;
        event.validate().ok()?;
        if event.request_id.is_some() {
            return None;
        }
        Some(event)
    } else {
        let name = value.get("hook_event_name")?.as_str()?;
        normalize(Agent::Claude, name, &value).ok().flatten()
    }
}

async fn handle(app: AppHandle, mut pipe: NamedPipeServer) {
    let Ok(Some(bytes)) = tokio::time::timeout(Duration::from_secs(1), read_line(&mut pipe)).await
    else {
        return;
    };
    let Some(mut event) = decode(&bytes) else {
        log::line("invalid agent event discarded");
        return;
    };
    // These sessions already stream through the chat adapter. Do not relay a
    // second tool card or permission channel through globally configured hooks.
    if app
        .state::<crate::agent_chat::AgentChat>()
        .owns_session(event.agent.as_str(), &event.session_id)
    {
        return;
    }
    let pending = app.state::<Pending>();
    let session = event.session_key();
    revoke_pending_for_event(&pending, &event);
    if app
        .state::<Shared>()
        .paused
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return;
    }
    if !event.requires_approval {
        let _ = app.emit_to(WINDOW_LABEL, "agent-hook", &event);
        return;
    }
    let Some((id, mut receiver)) = pending.insert(session, event.turn_id.clone(), Instant::now())
    else {
        event.requires_approval = false;
        let _ = app.emit_to(WINDOW_LABEL, "agent-hook", &event);
        return;
    };
    event.request_id = Some(id.clone());
    let Some(deadline) = pending.deadline(&id) else {
        return;
    };
    let _ = app.emit_to(WINDOW_LABEL, "agent-hook", &event);
    let decision = tokio::select! {
        decision = wait_for_decision(&mut receiver, deadline) => decision,
        _ = pipe.read_u8() => None,
    };
    pending.cancel(&id);
    if decision.is_none() {
        let _ = app.emit_to(WINDOW_LABEL, "approval-expired", json!({"requestId": id}));
    }
    if let Some(decision) = decision {
        let output = format!("{decision}\n");
        let _ = tokio::time::timeout(
            Duration::from_millis(300),
            pipe.write_all(output.as_bytes()),
        )
        .await;
    }
}

async fn wait_for_decision(
    receiver: &mut mpsc::Receiver<Reply>,
    deadline: Instant,
) -> Option<String> {
    let ack_deadline = tokio::time::Instant::from_std((Instant::now() + ACK_TIMEOUT).min(deadline));
    if !matches!(
        tokio::time::timeout_at(ack_deadline, receiver.recv()).await,
        Ok(Some(Reply::Ack))
    ) || Instant::now() >= deadline
    {
        return None;
    }
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), receiver.recv()).await {
        Ok(Some(Reply::Decision(decision)))
            if Instant::now() < deadline && matches!(decision.as_str(), "allow" | "deny") =>
        {
            Some(decision)
        }
        _ => None,
    }
}

fn revoke_pending_for_event(pending: &Pending, event: &AgentEvent) {
    let session = event.session_key();
    match event.event_type {
        AgentEventType::SessionEnded | AgentEventType::TurnStarted => {
            pending.cancel_session(&session);
        }
        AgentEventType::Interrupted | AgentEventType::TurnFinished | AgentEventType::TurnFailed => {
            pending.cancel_turn(&session, event.turn_id.as_deref());
        }
        _ => {}
    }
}

pub fn acknowledge(app: &AppHandle, id: &str) {
    app.state::<Pending>().acknowledge(id, Instant::now());
}
pub fn decline(app: &AppHandle, id: &str) {
    app.state::<Pending>().cancel(id);
    let _ = app.emit_to(WINDOW_LABEL, "approval-expired", json!({"requestId": id}));
}
pub fn cancel_all(app: &AppHandle) {
    app.state::<Pending>().cancel_all();
}
pub fn answer(app: &AppHandle, id: &str, decision: &str) -> bool {
    let accepted = app.state::<Pending>().decide(id, decision, Instant::now());
    if !accepted {
        decline(app, id);
    }
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pipe_disconnect_cancels_an_acknowledged_request() {
        use tokio::net::windows::named_pipe::ClientOptions;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let name = format!(
                r"\\.\pipe\coucou-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let mut server = create_pipe(&name, true).unwrap();
            assert!(create_pipe(&name, true).is_err());
            let mut client = ClientOptions::new().open(&name).unwrap();
            server.connect().await.unwrap();
            client
                .write_all(
                    b"{\"hook_event_name\":\"SessionStart\",\"session_id\":\"native-test\"}\n",
                )
                .await
                .unwrap();
            let bytes = read_line(&mut server).await.unwrap();
            assert_eq!(decode(&bytes).unwrap().session_id, "native-test");
            let pending = Pending::default();
            let now = Instant::now();
            let (id, mut receiver) = pending.insert("native-test".into(), None, now).unwrap();
            pending.acknowledge(&id, now);
            drop(client);
            let decision = tokio::select! {
                result = wait_for_decision(&mut receiver, now + Duration::from_secs(1)) => result,
                _ = server.read_u8() => None,
            };
            pending.cancel(&id);
            assert!(decision.is_none());
            assert!(!pending.decide(&id, "allow", Instant::now()));
        });
    }
    fn native_event(hook: &str, turn_id: Option<&str>) -> AgentEvent {
        let payload = json!({
            "hook_event_name": hook,
            "session_id": "claude-session",
            "turn_id": turn_id,
            "tool_name": "Bash",
            "tool_input": {"command": "dir"}
        });
        decode(&serde_json::to_vec(&payload).unwrap()).unwrap()
    }

    #[test]
    fn turn_boundaries_revoke_pending_requests_without_a_frontend() {
        for event_type in [
            AgentEventType::TurnStarted,
            AgentEventType::TurnFinished,
            AgentEventType::TurnFailed,
            AgentEventType::SessionEnded,
            AgentEventType::Interrupted,
        ] {
            let pending = Pending::default();
            let now = Instant::now();
            let request = native_event("PermissionRequest", Some("B"));
            let (id, mut rx) = pending
                .insert(request.session_key(), request.turn_id, now)
                .unwrap();
            pending.acknowledge(&id, now);
            let mut event = native_event("Stop", Some("B"));
            event.event_type = event_type;
            if matches!(
                event_type,
                AgentEventType::TurnStarted | AgentEventType::SessionEnded
            ) {
                event.turn_id = Some("another-turn".into());
            }
            revoke_pending_for_event(&pending, &event);
            assert!(!pending.decide(&id, "allow", now));
            assert!(matches!(rx.try_recv(), Ok(Reply::Ack)));
            assert!(matches!(rx.try_recv(), Ok(Reply::Decline)));
        }
    }

    #[test]
    fn late_stop_from_a_cannot_revoke_an_acknowledged_request_for_b() {
        let pending = Pending::default();
        let now = Instant::now();
        let request = native_event("PermissionRequest", Some("B"));
        assert!(request.requires_approval);
        let (id, mut rx) = pending
            .insert(request.session_key(), request.turn_id, now)
            .unwrap();
        assert!(pending.acknowledge(&id, now));
        assert!(matches!(rx.try_recv(), Ok(Reply::Ack)));

        // Use the same decoded native events and lifecycle helper as handle().
        revoke_pending_for_event(&pending, &native_event("Stop", Some("A")));
        assert!(pending.deadline(&id).is_some());
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        revoke_pending_for_event(&pending, &native_event("Notification", Some("A")));
        assert!(pending.deadline(&id).is_some());

        revoke_pending_for_event(&pending, &native_event("Stop", Some("B")));
        assert!(pending.deadline(&id).is_none());
        assert!(matches!(rx.try_recv(), Ok(Reply::Decline)));
        assert!(!pending.decide(&id, "allow", now));
    }

    #[test]
    fn missing_turn_ids_conservatively_restore_the_native_prompt() {
        for (request_turn, event_turn) in [(Some("B"), None), (None, Some("A")), (None, None)] {
            let pending = Pending::default();
            let now = Instant::now();
            let request = native_event("PermissionRequest", request_turn);
            let (id, mut rx) = pending
                .insert(request.session_key(), request.turn_id, now)
                .unwrap();
            assert!(pending.acknowledge(&id, now));
            revoke_pending_for_event(&pending, &native_event("Stop", event_turn));
            assert!(pending.deadline(&id).is_none());
            assert!(!pending.decide(&id, "allow", now));
            assert!(matches!(rx.try_recv(), Ok(Reply::Ack)));
            assert!(matches!(rx.try_recv(), Ok(Reply::Decline)));
        }
    }
    #[test]
    fn legacy_relay_is_normalized_and_sender_cannot_forge_request_ids() {
        let raw = json!({"hook_event_name":"SessionStart", "session_id":"s1", "cwd":"D:/project"});
        let event = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
        assert_eq!(event.agent, Agent::Claude);
        let mut wire = serde_json::to_value(event).unwrap();
        wire["requestId"] = json!("forged");
        assert!(decode(&serde_json::to_vec(&wire).unwrap()).is_none());
        assert!(decode(b"{}").is_none());
    }
    #[test]
    fn oversize_frames_and_missing_ack_never_allow() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let bytes = vec![b'x'; MAX_PAYLOAD + 1];
            assert!(read_line(&mut bytes.as_slice()).await.is_none());
            let (tx, mut rx) = mpsc::channel(1);
            tx.send(Reply::Decision("allow".into())).await.unwrap();
            assert!(
                wait_for_decision(&mut rx, Instant::now() + Duration::from_secs(1))
                    .await
                    .is_none()
            );
            let (tx, mut rx) = mpsc::channel(2);
            tx.send(Reply::Ack).await.unwrap();
            tx.send(Reply::Decision("allow".into())).await.unwrap();
            assert!(wait_for_decision(&mut rx, Instant::now()).await.is_none());
        });
    }
}
