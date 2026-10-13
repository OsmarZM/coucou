//! Import cancellation belongs to its initiating window, independently of model turns.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Default)]
struct Inner {
    active: HashMap<String, (String, watch::Sender<bool>)>,
    retired: VecDeque<String>,
}
#[derive(Clone, Default)]
pub struct ImportOperations(Arc<Mutex<Inner>>);
pub struct ImportLease {
    operations: ImportOperations,
    id: String,
    pub receiver: watch::Receiver<bool>,
}
impl ImportOperations {
    pub fn begin(&self, id: String, owner: &str) -> Result<ImportLease, String> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("Identificador de importação inválido.".into());
        }
        let mut inner = self.0.lock().map_err(|_| "Importações indisponíveis.")?;
        if inner.active.contains_key(&id) || inner.retired.contains(&id) {
            return Err(
                "Esta importação já foi iniciada. Atualize os anexos antes de tentar novamente."
                    .into(),
            );
        }
        if inner.active.len() >= 16 {
            return Err("Há muitas importações em andamento.".into());
        }
        let (sender, receiver) = watch::channel(false);
        inner.active.insert(id.clone(), (owner.into(), sender));
        Ok(ImportLease {
            operations: self.clone(),
            id,
            receiver,
        })
    }
    pub fn cancel(&self, id: &str, owner: &str) -> bool {
        self.0
            .lock()
            .ok()
            .and_then(|inner| {
                inner
                    .active
                    .get(id)
                    .filter(|(o, _)| o == owner)
                    .map(|(_, s)| s.send(true).is_ok())
            })
            .unwrap_or(false)
    }
    pub fn cancel_all(&self) {
        if let Ok(inner) = self.0.lock() {
            for (_, sender) in inner.active.values() {
                let _ = sender.send(true);
            }
        }
    }
}
impl Drop for ImportLease {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.operations.0.lock() {
            inner.active.remove(&self.id);
            inner.retired.push_back(self.id.clone());
            while inner.retired.len() > 256 {
                inner.retired.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_owned_and_ids_are_consumed() {
        let operations = ImportOperations::default();
        let lease = operations.begin("import-1".into(), "island").unwrap();
        assert!(!operations.cancel("import-1", "settings"));
        assert!(!*lease.receiver.borrow());
        assert!(operations.begin("import-1".into(), "island").is_err());
        assert!(operations.cancel("import-1", "island"));
        assert!(*lease.receiver.borrow());
        drop(lease);
        assert!(!operations.cancel("import-1", "island"));
        assert!(operations.begin("import-1".into(), "island").is_err());
    }
    #[test]
    fn pause_cancels_all_and_drop_releases_capacity() {
        let operations = ImportOperations::default();
        let leases: Vec<_> = (0..16)
            .map(|i| operations.begin(format!("import-{i}"), "island").unwrap())
            .collect();
        assert!(operations.begin("overflow".into(), "island").is_err());
        operations.cancel_all();
        assert!(leases.iter().all(|l| *l.receiver.borrow()));
        drop(leases);
        assert!(operations.begin("next".into(), "island").is_ok());
        assert!(operations.begin("../bad".into(), "island").is_err());
    }
}
