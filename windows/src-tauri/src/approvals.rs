// Pending requests are scoped to this backend instance and consumed once.
// Only the transport creates them; missing/expired requests never grant access.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

pub const REQUEST_LIFETIME: Duration = Duration::from_secs(109);
const MAX_PENDING: usize = 1;

#[derive(Debug)]
pub enum Reply {
    Ack,
    Decision(String),
    Decline,
}

struct Entry {
    session: String,
    turn_id: Option<String>,
    deadline: Instant,
    sender: mpsc::Sender<Reply>,
    acknowledged: bool,
}

pub struct Pending {
    epoch: String,
    counter: AtomicU64,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            epoch: format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            counter: AtomicU64::new(1),
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl Pending {
    pub fn insert(
        &self,
        session: String,
        turn_id: Option<String>,
        now: Instant,
    ) -> Option<(String, mpsc::Receiver<Reply>)> {
        let mut entries = self.entries.lock().ok()?;
        entries.retain(|_, entry| entry.deadline > now && !entry.sender.is_closed());
        if entries.len() >= MAX_PENDING {
            return None;
        }
        let id = format!(
            "{}-{}",
            self.epoch,
            self.counter.fetch_add(1, Ordering::Relaxed)
        );
        let (sender, receiver) = mpsc::channel(4);
        entries.insert(
            id.clone(),
            Entry {
                session,
                turn_id,
                deadline: now + REQUEST_LIFETIME,
                sender,
                acknowledged: false,
            },
        );
        Some((id, receiver))
    }

    pub fn deadline(&self, id: &str) -> Option<Instant> {
        self.entries
            .lock()
            .ok()?
            .get(id)
            .map(|entry| entry.deadline)
    }

    pub fn acknowledge(&self, id: &str, now: Instant) -> bool {
        let Ok(mut entries) = self.entries.lock() else {
            return false;
        };
        let Some(entry) = entries.get_mut(id) else {
            return false;
        };
        if entry.deadline <= now || entry.acknowledged {
            return false;
        }
        if entry.sender.try_send(Reply::Ack).is_err() {
            return false;
        }
        entry.acknowledged = true;
        true
    }

    pub fn decide(&self, id: &str, decision: &str, now: Instant) -> bool {
        if !matches!(decision, "allow" | "deny") {
            return false;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return false;
        };
        let Some(entry) = entries.remove(id) else {
            return false;
        };
        if entry.deadline <= now || !entry.acknowledged {
            return false;
        }
        entry
            .sender
            .try_send(Reply::Decision(decision.to_owned()))
            .is_ok()
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.remove(id) {
                let _ = entry.sender.try_send(Reply::Decline);
            }
        }
    }

    pub fn cancel_session(&self, session: &str) {
        self.cancel_turn(session, None);
    }

    /// An old terminal event cannot revoke a request from a known newer turn.
    /// Missing IDs preserve the conservative fallback to the native prompt.
    pub fn cancel_turn(&self, session: &str, turn_id: Option<&str>) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|_, entry| {
                if entry.session != session {
                    return true;
                }
                if let (Some(request_turn), Some(event_turn)) = (entry.turn_id.as_deref(), turn_id)
                {
                    if request_turn != event_turn {
                        return true;
                    }
                }
                let _ = entry.sender.try_send(Reply::Decline);
                false
            });
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            for (_, entry) in entries.drain() {
                let _ = entry.sender.try_send(Reply::Decline);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_is_acknowledged_scoped_and_consumed_once() {
        let pending = Pending::default();
        let now = Instant::now();
        let (id, mut rx) = pending
            .insert("[claude,session]".into(), None, now)
            .unwrap();
        assert!(!pending.decide("other-request", "allow", now));
        assert!(pending.acknowledge(&id, now));
        assert!(!pending.acknowledge(&id, now));
        assert!(pending.decide(&id, "allow", now));
        assert!(!pending.decide(&id, "allow", now));
        assert!(matches!(rx.try_recv(), Ok(Reply::Ack)));
        assert!(matches!(rx.try_recv(), Ok(Reply::Decision(word)) if word == "allow"));
    }

    #[test]
    fn expiry_restart_and_missing_ack_never_allow() {
        let pending = Pending::default();
        let now = Instant::now();
        let (id, _) = pending.insert("s".into(), None, now).unwrap();
        assert!(!pending.decide(&id, "allow", now));
        let (id, _rx) = pending.insert("s".into(), None, now).unwrap();
        assert!(pending.acknowledge(&id, now));
        assert!(!pending.decide(&id, "allow", now + REQUEST_LIFETIME));
        assert!(!Pending::default().decide(&id, "allow", now));
    }

    #[test]
    fn absolute_deadline_and_closed_receiver_reject_decisions() {
        let pending = Pending::default();
        let now = Instant::now();
        let (id, rx) = pending.insert("s".into(), None, now).unwrap();
        assert!(pending.acknowledge(&id, now));
        let deadline = pending.deadline(&id).unwrap();
        assert_eq!(deadline, now + REQUEST_LIFETIME);
        assert!(!pending.decide(&id, "allow", deadline));
        drop(rx);
        let (id, rx) = pending.insert("s".into(), None, now).unwrap();
        assert!(pending.acknowledge(&id, now));
        drop(rx);
        assert!(!pending.decide(&id, "allow", deadline - Duration::from_millis(1)));
    }

    #[test]
    fn second_request_defers_and_session_cancel_releases_native_flow() {
        let pending = Pending::default();
        let now = Instant::now();
        let (id, mut rx) = pending.insert("claude:s1".into(), None, now).unwrap();
        assert!(pending.insert("claude:s2".into(), None, now).is_none());
        pending.cancel_session("codex:s1");
        assert!(pending.acknowledge(&id, now));
        pending.cancel_session("claude:s1");
        assert!(matches!(rx.try_recv(), Ok(Reply::Ack)));
        assert!(matches!(rx.try_recv(), Ok(Reply::Decline)));
        assert!(!pending.decide(&id, "allow", now));
        assert!(pending.insert("claude:s2".into(), None, now).is_some());
    }
}
