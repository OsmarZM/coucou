use super::{guards::*, *};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use std::collections::HashSet;

const SESSION_COLUMNS: &str = "c.id,c.agent,c.session_id,c.cwd,c.title,(SELECT COUNT(*) FROM messages m WHERE m.conversation_id=c.id),c.updated_at,(SELECT b.context_id FROM personal_context_bindings b WHERE b.conversation_id=c.id)";
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

impl MemoryService {
    /// Store only complete messages from the local Coucou pipeline. Sensitive
    /// content is skipped before entering SQLite or its FTS index.
    pub fn persist_conversation_message(
        &self,
        message: &ConversationMessage,
    ) -> Result<HistoryWrite, String> {
        validate_id(&message.id)?;
        validate_id(&message.conversation_id)?;
        validate_id(&message.agent)?;
        if let Some(id) = &message.session_id {
            validate_id(id)?;
        }
        if let Some(cwd) = &message.cwd {
            if cwd.len() > 4096 || cwd.chars().any(char::is_control) {
                return Err("Pasta do histórico inválida.".into());
            }
        }
        if message.content.trim().is_empty()
            || message.content.len() > MAX_MESSAGE_BYTES
            || message.content.contains('\0')
            || message.content.contains('\u{1b}')
        {
            return Err("Mensagem vazia, inválida ou acima de 65536 bytes.".into());
        }
        if let Some(title) = &message.title {
            if title.trim().is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
                return Err("Título do histórico inválido.".into());
            }
        }
        if [
            &message.content,
            &message.agent,
            &message.id,
            &message.conversation_id,
        ]
        .iter()
        .any(|value| contains_sensitive_content(value))
            || [&message.session_id, &message.cwd, &message.title]
                .iter()
                .any(|value| {
                    value
                        .as_ref()
                        .is_some_and(|value| contains_sensitive_content(value))
                })
        {
            return Ok(skipped(
                "Conteúdo possivelmente sensível não foi persistido.",
            ));
        }
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        if !preferences_in(&tx)?.persist_history {
            return Ok(skipped("Persistência de histórico desativada."));
        }
        if message
            .cwd
            .as_ref()
            .is_some_and(|cwd| !cwd.trim().is_empty())
        {
            let bound: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM personal_context_bindings WHERE conversation_id=?1)",
                [&message.conversation_id], |row| row.get(0)
            ).map_err(db_error)?;
            if bound {
                return Err("Uma conversa pessoal não pode receber histórico de projeto.".into());
            }
        }
        let existing = tx
            .query_row(
                "SELECT conversation_id,role,content FROM messages WHERE id=?1",
                [&message.id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        if let Some((conversation, role, content)) = existing {
            if conversation == message.conversation_id
                && role == message.role.as_str()
                && content == message.content
            {
                return Ok(HistoryWrite {
                    persisted: true,
                    duplicate: true,
                    reason: None,
                });
            }
            return Err("Identificador de mensagem já utilizado com outro conteúdo; nenhum registro foi substituído.".into());
        }
        ensure_capacity(&tx, "messages")?;
        let existing = tx
            .query_row(
                "SELECT agent,session_id FROM conversations WHERE id=?1",
                [&message.conversation_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        let timestamp = now();
        if let Some((agent, session_id)) = existing {
            if agent != message.agent
                || (session_id.is_some()
                    && message.session_id.is_some()
                    && session_id != message.session_id)
            {
                return Err("A conversa pertence a outro agente ou sessão; abra outra conversa para trocar de sessão.".into());
            }
            tx.execute("UPDATE conversations SET session_id=COALESCE(session_id,?1),cwd=COALESCE(?2,cwd),title=COALESCE(?3,title),updated_at=?4 WHERE id=?5", params![message.session_id,message.cwd,message.title,timestamp,message.conversation_id]).map_err(db_error)?;
        } else {
            ensure_capacity(&tx, "conversations")?;
            tx.execute("INSERT INTO conversations(id,agent,session_id,cwd,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)", params![message.conversation_id,message.agent,message.session_id,message.cwd,message.title.as_deref().unwrap_or("Conversa"),timestamp]).map_err(db_error)?;
        }
        tx.execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES(?1,?2,?3,?4,?5)", params![message.id,message.conversation_id,message.role.as_str(),message.content,timestamp]).map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(HistoryWrite {
            persisted: true,
            duplicate: false,
            reason: None,
        })
    }

    pub fn sessions_list(&self, requested_limit: usize) -> Result<Vec<SessionRecord>, String> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!("SELECT {SESSION_COLUMNS} FROM conversations c ORDER BY c.updated_at DESC,c.id LIMIT ?1")).map_err(db_error)?;
        let result = statement
            .query_map([limit(requested_limit)], map_session)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        Ok(result)
    }

    /// FTS operators are never accepted as executable query syntax. One hit
    /// per conversation avoids a long transcript crowding out other sessions.
    pub fn sessions_search(
        &self,
        query: &str,
        requested_limit: usize,
    ) -> Result<Vec<SessionSearchHit>, String> {
        let expression = search_expression(query)?;
        if expression.is_empty() {
            return Ok(Vec::new());
        }
        let connection = self.lock()?;
        let mut statement = connection.prepare(&format!("SELECT {SESSION_COLUMNS},m.id,snippet(message_fts,0,'','','…',24) FROM message_fts JOIN messages m ON m.rowid=message_fts.rowid JOIN conversations c ON c.id=m.conversation_id WHERE message_fts MATCH ?1 ORDER BY bm25(message_fts),c.updated_at DESC,m.id LIMIT 1000")).map_err(db_error)?;
        let rows = statement
            .query_map([expression], |row| {
                Ok(SessionSearchHit {
                    session: map_session(row)?,
                    message_id: row.get(8)?,
                    excerpt: row.get(9)?,
                })
            })
            .map_err(db_error)?;
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        for row in rows {
            let row = row.map_err(db_error)?;
            if seen.insert(row.session.conversation_id.clone()) {
                result.push(row);
                if result.len() >= limit(requested_limit) as usize {
                    break;
                }
            }
        }
        Ok(result)
    }

    pub fn conversation_messages(
        &self,
        conversation_id: &str,
        requested_limit: usize,
    ) -> Result<Vec<StoredMessage>, String> {
        validate_id(conversation_id)?;
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT id,conversation_id,role,content,created_at FROM (SELECT rowid,id,conversation_id,role,content,created_at FROM messages WHERE conversation_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT ?2) ORDER BY created_at,rowid").map_err(db_error)?;
        let result = statement
            .query_map(
                params![conversation_id, limit(requested_limit)],
                map_message,
            )
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        Ok(result)
    }

    /// Forgetting local history does not delete the independent CLI/provider
    /// transcript or revoke anything previously sent to that provider.
    pub fn forget_conversation(&self, conversation_id: &str) -> Result<usize, String> {
        validate_id(conversation_id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let count: usize = tx
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id=?1",
                [conversation_id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        tx.execute("DELETE FROM conversations WHERE id=?1", [conversation_id])
            .map_err(db_error)?;
        tx.execute(
            "DELETE FROM personal_context_bindings WHERE conversation_id=?1",
            [conversation_id],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(count)
    }
}

fn skipped(reason: &str) -> HistoryWrite {
    HistoryWrite {
        persisted: false,
        duplicate: false,
        reason: Some(reason.into()),
    }
}

fn map_session(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        conversation_id: row.get(0)?,
        agent: row.get(1)?,
        session_id: row.get(2)?,
        cwd: row.get(3)?,
        title: row.get(4)?,
        message_count: row.get(5)?,
        updated_at: row.get(6)?,
        context_id: row.get(7)?,
    })
}
fn map_message(row: &Row<'_>) -> rusqlite::Result<StoredMessage> {
    Ok(StoredMessage {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: MessageRole::parse(&row.get::<_, String>(2)?).map_err(conversion_error)?,
        content: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub(crate) fn all(
    connection: &Connection,
) -> Result<(Vec<SessionRecord>, Vec<StoredMessage>), String> {
    let mut sessions = connection
        .prepare(&format!(
            "SELECT {SESSION_COLUMNS} FROM conversations c ORDER BY c.created_at,c.id"
        ))
        .map_err(db_error)?;
    let sessions = sessions
        .query_map([], map_session)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let mut messages = connection.prepare("SELECT id,conversation_id,role,content,created_at FROM messages ORDER BY created_at,rowid").map_err(db_error)?;
    let messages = messages
        .query_map([], map_message)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok((sessions, messages))
}
