//! Durable provenance and one-use dispatch receipts, independent of history consent.
use super::*;
use sha2::{Digest, Sha256};

impl MemoryService {
    pub fn native_session_owned(
        &self,
        conversation: &str,
        agent: &str,
        session: &str,
    ) -> Result<bool, String> {
        let connection = self.lock()?;
        dispatch_schema(&connection)?;
        connection.query_row("SELECT EXISTS(SELECT 1 FROM native_session_owners WHERE conversation_id=?1 AND agent=?2 AND session_id=?3)",params![conversation,agent,session],|row|row.get(0)).map_err(db_error)
    }
    pub fn register_native_session(
        &self,
        conversation: &str,
        agent: &str,
        session: &str,
    ) -> Result<(), String> {
        validate_id(conversation)?;
        validate_id(session)?;
        if !["codex", "claude", "gemini", "copilot"].contains(&agent) {
            return Err("Agente inválido.".into());
        }
        let connection = self.lock()?;
        dispatch_schema(&connection)?;
        connection.execute("INSERT OR IGNORE INTO native_session_owners(conversation_id,agent,session_id) VALUES(?1,?2,?3)",params![conversation,agent,session]).map_err(db_error)?;
        Ok(())
    }
    pub fn reserve_dispatch(
        &self,
        conversation: &str,
        run: &str,
        agent: &str,
        session: Option<&str>,
        message: &str,
        external: bool,
    ) -> Result<(), String> {
        validate_id(conversation)?;
        validate_id(run)?;
        let mut connection = self.lock()?;
        dispatch_schema(&connection)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let digest = format!(
            "{:x}",
            Sha256::digest(message.replace("\r\n", "\n").trim().as_bytes())
        );
        let stamp = now();
        if external {
            let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM dispatch_receipts WHERE agent=?1 AND session_id=?2 AND content_hash=?3 AND created_at>?4 AND status IN ('pending','sending','completed','failed'))",params![agent,session,digest,stamp-60_000],|row|row.get(0)).map_err(db_error)?;
            if duplicate {
                return Err("Envio duplicado bloqueado. A mesma mensagem já foi preparada ou enviada a esta sessão há menos de um minuto. Nenhum reenvio automático será feito.".into());
            }
        }
        let inserted=tx.execute("INSERT OR IGNORE INTO dispatch_receipts(run_id,conversation_id,agent,session_id,content_hash,created_at,status) VALUES(?1,?2,?3,?4,?5,?6,'pending')",params![run,conversation,agent,session,digest,stamp]).map_err(db_error)?;
        if inserted != 1 {
            return Err("Este envio já foi utilizado. Prepare uma nova mensagem e revise a autorização novamente.".into());
        }
        // Receipts contain hashes/IDs, never message contents. Keep a bounded audit window.
        tx.execute(
            "DELETE FROM dispatch_receipts WHERE created_at<?1",
            [stamp - 30 * 24 * 60 * 60 * 1000],
        )
        .map_err(db_error)?;
        let count: i64 = tx
            .query_row("SELECT COUNT(*) FROM dispatch_receipts", [], |row| {
                row.get(0)
            })
            .map_err(db_error)?;
        if count > 50_000 {
            return Err(
                "Limite de comprovantes de envio atingido. Nenhum novo envio foi liberado.".into(),
            );
        }
        tx.commit().map_err(db_error)
    }
    pub fn dispatch_status(&self, run: &str, status: &str) -> Result<(), String> {
        if !["sending", "completed", "failed", "interrupted", "declined"].contains(&status) {
            return Err("Estado de envio inválido.".into());
        }
        let connection = self.lock()?;
        dispatch_schema(&connection)?;
        connection
            .execute(
                "UPDATE dispatch_receipts SET status=?2 WHERE run_id=?1",
                params![run, status],
            )
            .map_err(db_error)?;
        Ok(())
    }
}
fn dispatch_schema(connection: &Connection) -> Result<(), String> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS native_session_owners(conversation_id TEXT NOT NULL,agent TEXT NOT NULL,session_id TEXT NOT NULL,PRIMARY KEY(conversation_id,agent,session_id));CREATE TABLE IF NOT EXISTS dispatch_receipts(run_id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL,agent TEXT NOT NULL,session_id TEXT,content_hash TEXT NOT NULL,created_at INTEGER NOT NULL,status TEXT NOT NULL);CREATE INDEX IF NOT EXISTS dispatch_duplicates ON dispatch_receipts(agent,session_id,content_hash,created_at);").map_err(db_error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_provenance_never_becomes_ownership() {
        let memory = MemoryService::ephemeral().unwrap();
        assert!(!memory
            .native_session_owned("c", "codex", "external")
            .unwrap());
        memory
            .reserve_dispatch("c", "r", "codex", Some("external"), "Olá", true)
            .unwrap();
        memory.dispatch_status("r", "completed").unwrap();
        assert!(!memory
            .native_session_owned("c", "codex", "external")
            .unwrap());
        memory.register_native_session("c", "codex", "new").unwrap();
        assert!(memory.native_session_owned("c", "codex", "new").unwrap());
        assert!(!memory
            .native_session_owned("other", "codex", "new")
            .unwrap());
    }
    #[test]
    fn replay_and_cross_conversation_duplicates_are_blocked() {
        let memory = MemoryService::ephemeral().unwrap();
        memory
            .reserve_dispatch("c", "r", "codex", Some("s"), "Mensagem", true)
            .unwrap();
        assert!(memory
            .reserve_dispatch("other", "r2", "codex", Some("s"), "Mensagem", true)
            .is_err());
        memory.dispatch_status("r", "declined").unwrap();
        assert!(memory
            .reserve_dispatch("c", "r", "codex", Some("s"), "Outra", true)
            .is_err());
        memory
            .reserve_dispatch("c", "r3", "codex", Some("s"), "Mensagem", true)
            .unwrap();
    }
}
