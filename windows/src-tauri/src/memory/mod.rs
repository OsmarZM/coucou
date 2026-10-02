//! Coucou-owned personal memory. Local concepts inspired by Hermes; no Hermes
//! source code is copied. Memories and skills carry context, never authorization.
//! All mutations pass through one serialized connection and an SQLite transaction.

mod dispatch;
mod guards;
mod history;
mod models;
mod personal_context;
mod schema;
mod skills;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use guards::contains_sensitive_content;
pub use models::*;
pub use personal_context::PERSONAL_CONTEXT_ID;

use guards::*;
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MEMORY_COLUMNS: &str = "id,kind,memory_key,content,scope,source,confidence,state,revision,replaces_id,base_revision,created_at,updated_at";
const MAX_ITEMS: i64 = 10_000;
static IDS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct MemoryService {
    connection: Arc<Mutex<Connection>>,
}

impl MemoryService {
    pub fn new() -> Result<Self, String> {
        Self::open(
            crate::settings::local_dir()
                .join("personal")
                .join("memory.sqlite3"),
        )
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Não foi possível preparar a pasta de memória.".to_string())?;
        }
        let mut connection = Connection::open(path).map_err(db_error)?;
        Self::prepare(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Useful for an explicitly ephemeral profile and isolated tests.
    pub fn ephemeral() -> Result<Self, String> {
        let mut connection = Connection::open_in_memory().map_err(db_error)?;
        Self::prepare(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn prepare(connection: &mut Connection) -> Result<(), String> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON;").map_err(db_error)?;
        schema::migrate(connection)
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "Estado de memória indisponível; nenhuma alteração foi aplicada.".into())
    }

    pub fn preferences(&self) -> Result<MemoryPreferences, String> {
        let connection = self.lock()?;
        preferences_in(&connection)
    }

    /// Compatibility with older clients. Learning/history are now part of the
    /// continuous personal conversation, not independent UI permission toggles.
    pub fn set_preferences(
        &self,
        _preferences: MemoryPreferences,
    ) -> Result<MemoryPreferences, String> {
        self.enable_automatic_personal_context()
    }

    /// Idempotent boot migration of the current human-authorized policy.
    /// Permission grants and native-session provenance live elsewhere.
    pub fn enable_automatic_personal_context(&self) -> Result<MemoryPreferences, String> {
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        preferences_in(&tx)?;
        let preferences = MemoryPreferences::default();
        let json = serde_json::to_string(&preferences)
            .map_err(|_| "Preferência de memória inválida.".to_string())?;
        tx.execute("UPDATE preferences SET json=?1 WHERE id=1", [json])
            .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(preferences)
    }

    /// Always proposes a candidate. An approved value remains active while a
    /// replacement awaits review; duplicate proposals do not create new versions.
    pub fn upsert_candidate(&self, candidate: MemoryCandidate) -> Result<MemoryRecord, String> {
        self.upsert_candidate_inner(candidate, false)
    }
    fn upsert_candidate_inner(
        &self,
        mut candidate: MemoryCandidate,
        auto: bool,
    ) -> Result<MemoryRecord, String> {
        validate_candidate(&candidate)?;
        candidate.key = candidate
            .key
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        candidate.content = candidate.content.trim().to_string();
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let preferences = preferences_in(&tx)?;
        if auto && (!preferences.learning_enabled || !preferences.auto_save_user_facts) {
            return Err("Registro automático de declarações não habilitado.".into());
        }
        if candidate.source.kind != SourceKind::User && !preferences.learning_enabled {
            return Err(
                "Aprendizado pessoal desativado; propostas automáticas não foram armazenadas."
                    .into(),
            );
        }
        let source =
            serde_json::to_string(&candidate.source).map_err(|_| "Origem inválida.".to_string())?;
        let timestamp = now();
        if let Some(id) = &candidate.id {
            let current = memory_by_id(&tx, id)?.ok_or("Memória não encontrada.")?;
            if current.state != ReviewState::Pending {
                return Err("Crie uma nova proposta para alterar uma memória já revisada.".into());
            }
            if candidate.expected_revision != Some(current.revision) {
                return Err(stale());
            }
            if current.kind != candidate.kind
                || current.key != candidate.key
                || current.scope != candidate.scope
            {
                return Err("A identidade da memória não pode mudar durante a edição.".into());
            }
            tx.execute("UPDATE memories SET content=?1,source=?2,confidence=?3,revision=revision+1,updated_at=?4 WHERE id=?5", params![candidate.content,source,candidate.confidence,timestamp,id]).map_err(db_error)?;
            let record = memory_by_id(&tx, id)?.ok_or("Memória não encontrada após a edição.")?;
            tx.commit().map_err(db_error)?;
            return Ok(record);
        }
        if candidate.expected_revision.is_some() {
            return Err("Uma revisão esperada exige o identificador da proposta.".into());
        }
        let active = active_memory(&tx, candidate.kind, &candidate.key, &candidate.scope)?;
        if let Some(active) = &active {
            if active.content == candidate.content {
                return Ok(active.clone());
            }
        }
        let pending = {
            let sql = format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE kind=?1 AND memory_key=?2 AND scope=?3 AND state='pending' AND content=?4 AND source=?5 AND confidence=?6 ORDER BY created_at DESC LIMIT 1");
            tx.query_row(
                &sql,
                params![
                    candidate.kind.as_str(),
                    candidate.key,
                    candidate.scope,
                    candidate.content,
                    source,
                    candidate.confidence
                ],
                map_memory,
            )
            .optional()
            .map_err(db_error)?
        };
        if let Some(pending) = pending {
            if pending.replaces_id == active.as_ref().map(|row| row.id.clone())
                && pending.base_revision == active.as_ref().map(|row| row.revision)
                && pending.source == candidate.source
            {
                if auto {
                    let approved = approve_in(&tx, pending, true)?;
                    tx.commit().map_err(db_error)?;
                    return Ok(approved);
                }
                return Ok(pending);
            }
        }
        ensure_capacity(&tx, "memories")?;
        let id = new_id("memory");
        tx.execute("INSERT INTO memories(id,kind,memory_key,content,scope,source,confidence,state,revision,replaces_id,base_revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'pending',1,?8,?9,?10,?10)", params![id,candidate.kind.as_str(),candidate.key,candidate.content,candidate.scope,source,candidate.confidence,active.as_ref().map(|row| &row.id),active.as_ref().map(|row| row.revision),timestamp]).map_err(db_error)?;
        let record = memory_by_id(&tx, &id)?.ok_or("Memória não encontrada após a gravação.")?;
        let record = if auto {
            approve_in(&tx, record, true)?
        } else {
            record
        };
        tx.commit().map_err(db_error)?;
        Ok(record)
    }

    /// An internal path for a direct low-risk user statement. Never
    /// expose this as a model tool accepting a self-declared `source=user`.
    pub fn capture_user_statement(
        &self,
        mut candidate: MemoryCandidate,
    ) -> Result<MemoryRecord, String> {
        if candidate.id.is_some() || candidate.expected_revision.is_some() {
            return Err("Registro automático só aceita uma nova declaração direta.".into());
        }
        if candidate.source.kind != SourceKind::User
            || candidate.kind == MemoryKind::Procedure
            || candidate.confidence != 1.0
        {
            return Err("Inferências e procedimentos precisam de revisão explícita.".into());
        }
        if !automatic_content_is_safe(
            &candidate.key,
            &candidate.content,
            &candidate.source.evidence,
        ) || !has_direct_low_risk_statement(candidate.kind, &candidate.content)
        {
            return Err(
                "Esta declaração requer revisão explícita antes de ser reutilizada.".into(),
            );
        }
        candidate.key = canonical_automatic_key(candidate.kind, &candidate.content)
            .ok_or("Declaração automática sem categoria de baixo risco.")?
            .into();
        // Validation, proposal and automatic curation share one transaction.
        self.upsert_candidate_inner(candidate, true)
    }

    /// Model proposals retain inference provenance. Only an explicit low-risk
    /// statement supported by the ORIGINAL user message can be auto-curated.
    /// Ambiguous observations and all procedures remain pending for inspection.
    pub fn curate_observed_statement(
        &self,
        mut candidate: MemoryCandidate,
        original_user_message: &str,
    ) -> Result<MemoryRecord, String> {
        if candidate.id.is_some()
            || candidate.expected_revision.is_some()
            || candidate.source.kind != SourceKind::Inference
        {
            return Err(
                "Observação automática exige uma nova inferência com origem preservada.".into(),
            );
        }
        candidate.source.evidence = if contains_sensitive_content(original_user_message) {
            "Observação desta conversa; conteúdo sensível excluído da evidência.".into()
        } else {
            bounded(original_user_message.trim(), 1024)
        };
        if candidate.source.evidence.trim().is_empty() {
            candidate.source.evidence =
                "Observação desta conversa sem declaração direta verificável.".into();
        }
        validate_candidate(&candidate)?;
        let auto = original_user_message.len() <= 1024
            && candidate.confidence >= 0.85
            && observed_statement_is_supported(&candidate);
        if auto {
            candidate.key = canonical_automatic_key(candidate.kind, original_user_message)
                .ok_or("Observação automática sem categoria de baixo risco.")?
                .into();
            candidate.content = original_user_message.trim().to_string();
        }
        self.upsert_candidate_inner(candidate, auto)
    }

    pub fn approve(
        &self,
        id: &str,
        expected_revision: u64,
        explicit_user_confirmation: bool,
    ) -> Result<MemoryRecord, String> {
        self.approve_inner(id, expected_revision, explicit_user_confirmation, false)
    }
    fn approve_inner(
        &self,
        id: &str,
        expected_revision: u64,
        confirmed: bool,
        auto: bool,
    ) -> Result<MemoryRecord, String> {
        if !confirmed {
            return Err("A aprovação exige confirmação explícita do usuário.".into());
        }
        validate_id(id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let record = memory_by_id(&tx, id)?.ok_or("Memória não encontrada.")?;
        if record.revision != expected_revision || record.state != ReviewState::Pending {
            return Err(stale());
        }
        let record = approve_in(&tx, record, auto)?;
        tx.commit().map_err(db_error)?;
        Ok(record)
    }

    pub fn reject(&self, id: &str, expected_revision: u64) -> Result<MemoryRecord, String> {
        validate_id(id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let record = memory_by_id(&tx, id)?.ok_or("Memória não encontrada.")?;
        if record.revision != expected_revision || record.state != ReviewState::Pending {
            return Err(stale());
        }
        tx.execute(
            "UPDATE memories SET state='rejected',revision=revision+1,updated_at=?1 WHERE id=?2",
            params![now(), id],
        )
        .map_err(db_error)?;
        let record = memory_by_id(&tx, id)?.ok_or("Memória não encontrada após a rejeição.")?;
        tx.commit().map_err(db_error)?;
        Ok(record)
    }

    pub fn list(&self, query: &MemoryQuery) -> Result<Vec<MemoryRecord>, String> {
        self.search(query)
    }
    pub fn search(&self, query: &MemoryQuery) -> Result<Vec<MemoryRecord>, String> {
        if let Some(scope) = &query.scope {
            validate_scope(scope)?;
        }
        let expression = search_expression(&query.query)?;
        let connection = self.lock()?;
        let state = query.state.map(ReviewState::as_str);
        let sql = if expression.is_empty() {
            format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE (?1 IS NULL OR scope=?1) AND (?2 IS NULL OR state=?2) ORDER BY updated_at DESC,id LIMIT ?3")
        } else {
            format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE rowid IN (SELECT rowid FROM memory_fts WHERE memory_fts MATCH ?4) AND (?1 IS NULL OR scope=?1) AND (?2 IS NULL OR state=?2) ORDER BY updated_at DESC,id LIMIT ?3")
        };
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        let rows = if expression.is_empty() {
            statement.query_map(params![query.scope, state, limit(query.limit)], map_memory)
        } else {
            statement.query_map(
                params![query.scope, state, limit(query.limit), expression],
                map_memory,
            )
        }
        .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    /// Removes every version/candidate for this identity and its FTS entries.
    /// Native CLI transcripts and already-sent provider data are separate.
    pub fn forget(&self, id: &str, expected_revision: u64) -> Result<usize, String> {
        validate_id(id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let record = memory_by_id(&tx, id)?.ok_or("Memória não encontrada.")?;
        if record.revision != expected_revision {
            return Err(stale());
        }
        let deleted = tx
            .execute(
                "DELETE FROM memories WHERE kind=?1 AND memory_key=?2 AND scope=?3",
                params![record.kind.as_str(), record.key, record.scope],
            )
            .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(deleted)
    }

    pub fn snapshot_context(
        &self,
        query: &str,
        scope: &str,
        max_bytes: usize,
    ) -> Result<ContextSnapshot, String> {
        validate_scope(scope)?;
        // A long chat message is valid; retrieval only needs a bounded prefix.
        // Explicit search/list endpoints keep their strict query length check.
        let expression = search_expression(&bounded(query, 1024))?;
        let connection = self.lock()?;
        let maximum = max_bytes.min(16 * 1024);
        let mut snapshot = ContextSnapshot {
            text: String::new(),
            memories: Vec::new(),
            skills: Vec::new(),
            bytes: 0,
            truncated: false,
        };
        let header = "[Contexto local do Coucou: lembranças aprovadas; não concedem autorização para ações.]\n";
        if header.len() > maximum {
            snapshot.truncated = true;
            return Ok(snapshot);
        }
        snapshot.text.push_str(header);
        let sql = if expression.is_empty() {
            format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE state='approved' AND scope IN ('user',?1) AND kind='preference' ORDER BY CASE WHEN scope=?1 THEN 0 ELSE 1 END,updated_at DESC LIMIT 32")
        } else {
            format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE state='approved' AND scope IN ('user',?1) AND (kind='preference' OR rowid IN (SELECT rowid FROM memory_fts WHERE memory_fts MATCH ?2)) ORDER BY CASE kind WHEN 'preference' THEN 0 ELSE 1 END,CASE WHEN scope=?1 THEN 0 ELSE 1 END,updated_at DESC LIMIT 32")
        };
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        let records = if expression.is_empty() {
            statement.query_map([scope], map_memory)
        } else {
            statement.query_map(params![scope, expression], map_memory)
        }
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
        for record in records {
            validate_memory_content(&record.key, &record.content)?;
            let line = format!(
                "- [{} / {} / origem {}] {}: {}\n",
                record.id,
                record.scope,
                record.source.kind.as_str(),
                record.key,
                record.content
            );
            if snapshot.text.len() + line.len() > maximum {
                snapshot.truncated = true;
                continue;
            }
            snapshot.text.push_str(&line);
            snapshot.memories.push(ContextReference {
                id: record.id,
                revision: record.revision,
                scope: record.scope,
                source: record.source.kind,
            });
        }
        skills::append_context(&connection, &expression, scope, maximum, &mut snapshot)?;
        snapshot.bytes = snapshot.text.len();
        Ok(snapshot)
    }

    /// Bounded JSON export. Selecting history is an explicit caller decision.
    pub fn export(&self, include_history: bool) -> Result<String, String> {
        let connection = self.lock()?;
        // Reserve a conservative JSON escaping/metadata budget before loading
        // all rows. Large stores require selective export, not an unbounded UI
        // allocation which could stall or exhaust the desktop process.
        let mut budget = export_estimate(
            &connection,
            "memories",
            "memory_key||content||scope||source",
        )? + export_estimate(
            &connection,
            "skills",
            "name||description||body||scope||source",
        )?;
        if include_history {
            budget += export_estimate(&connection, "messages", "content")?
                + export_estimate(
                    &connection,
                    "conversations",
                    "agent||COALESCE(session_id,'')||COALESCE(cwd,'')||title",
                )?;
        }
        if budget > 32 * 1024 * 1024 {
            return Err("Exportação acima de 32 MiB; exporte procedimentos individualmente ou exclua o histórico da exportação.".into());
        }
        let mut statement = connection
            .prepare(&format!(
                "SELECT {MEMORY_COLUMNS} FROM memories ORDER BY created_at,id"
            ))
            .map_err(db_error)?;
        let memories = statement
            .query_map([], map_memory)
            .map_err(db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_error)?;
        let skills = skills::all(&connection)?;
        let (sessions, messages) = if include_history {
            history::all(&connection)?
        } else {
            (Vec::new(), Vec::new())
        };
        let export = StoreExport {
            format_version: 1,
            exported_at: now(),
            preferences: preferences_in(&connection)?,
            memories,
            skills,
            sessions,
            messages,
        };
        let json = serde_json::to_string_pretty(&export)
            .map_err(|_| "Não foi possível exportar a memória.".to_string())?;
        if json.len() > 32 * 1024 * 1024 {
            return Err("Exportação acima de 32 MiB.".into());
        }
        Ok(json)
    }
}

fn approve_in(
    connection: &Connection,
    record: MemoryRecord,
    auto: bool,
) -> Result<MemoryRecord, String> {
    if auto {
        let preferences = preferences_in(connection)?;
        if !preferences.learning_enabled
            || !preferences.auto_save_user_facts
            || record.kind == MemoryKind::Procedure
            || !automatic_content_is_safe(&record.key, &record.content, &record.source.evidence)
            || match record.source.kind {
                SourceKind::User => {
                    record.confidence != 1.0
                        || !has_direct_low_risk_statement(record.kind, &record.content)
                }
                SourceKind::Inference => {
                    record.confidence < 0.85 || !observed_record_is_supported(&record)
                }
                SourceKind::Document | SourceKind::Import => true,
            }
        {
            return Err("Consentimento para aprendizado automático indisponível.".into());
        }
    }
    validate_memory_content(&record.key, &record.content)?;
    validate_source(&record.source)?;
    let active = active_memory(connection, record.kind, &record.key, &record.scope)?;
    if record.replaces_id != active.as_ref().map(|row| row.id.clone())
        || record.base_revision != active.as_ref().map(|row| row.revision)
    {
        return Err(
            "A origem aprovada mudou; revise uma nova proposta antes de substituir a memória."
                .into(),
        );
    }
    let timestamp = now();
    if let Some(active) = active {
        connection.execute("UPDATE memories SET state='superseded',revision=revision+1,updated_at=?1 WHERE id=?2", params![timestamp,active.id]).map_err(db_error)?;
    }
    connection
        .execute(
            "UPDATE memories SET state='approved',revision=revision+1,updated_at=?1 WHERE id=?2",
            params![timestamp, record.id],
        )
        .map_err(db_error)?;
    memory_by_id(connection, &record.id)?.ok_or("Memória não encontrada após a aprovação.".into())
}
fn export_estimate(connection: &Connection, table: &str, expression: &str) -> Result<u64, String> {
    // Table/expression are internal constants, never user SQL.
    let bytes: u64 = connection
        .query_row(
            &format!(
                "SELECT COALESCE(SUM(length(CAST({expression} AS BLOB))*6+2048),0) FROM {table}"
            ),
            [],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(bytes)
}

pub(crate) fn preferences_in(connection: &Connection) -> Result<MemoryPreferences, String> {
    let json: String = connection
        .query_row("SELECT json FROM preferences WHERE id=1", [], |row| {
            row.get(0)
        })
        .map_err(db_error)?;
    serde_json::from_str(&json).map_err(|_| {
        "Configuração de memória inválida; aprendizado e persistência não foram autorizados.".into()
    })
}
pub(crate) fn memory_by_id(
    connection: &Connection,
    id: &str,
) -> Result<Option<MemoryRecord>, String> {
    connection
        .query_row(
            &format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE id=?1"),
            [id],
            map_memory,
        )
        .optional()
        .map_err(db_error)
}
fn active_memory(
    connection: &Connection,
    kind: MemoryKind,
    key: &str,
    scope: &str,
) -> Result<Option<MemoryRecord>, String> {
    connection.query_row(&format!("SELECT {MEMORY_COLUMNS} FROM memories WHERE kind=?1 AND memory_key=?2 AND scope=?3 AND state='approved'"), params![kind.as_str(),key,scope], map_memory).optional().map_err(db_error)
}
pub(crate) fn map_memory(row: &Row<'_>) -> rusqlite::Result<MemoryRecord> {
    Ok(MemoryRecord {
        id: row.get(0)?,
        kind: MemoryKind::parse(&row.get::<_, String>(1)?).map_err(conversion_error)?,
        key: row.get(2)?,
        content: row.get(3)?,
        scope: row.get(4)?,
        source: serde_json::from_str(&row.get::<_, String>(5)?)
            .map_err(|_| conversion_error("Origem de memória inválida.".into()))?,
        confidence: row.get(6)?,
        state: ReviewState::parse(&row.get::<_, String>(7)?).map_err(conversion_error)?,
        revision: row.get(8)?,
        replaces_id: row.get(9)?,
        base_revision: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}
pub(crate) fn conversion_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}
pub(crate) fn ensure_capacity(connection: &Connection, table: &str) -> Result<(), String> {
    let count: i64 = connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(db_error)?;
    if count >= MAX_ITEMS {
        return Err(
            "Limite local atingido; exporte ou esqueça registros antes de gravar outros.".into(),
        );
    }
    Ok(())
}
pub(crate) fn limit(limit: usize) -> i64 {
    limit.clamp(1, 200) as i64
}
pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
pub(crate) fn new_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        IDS.fetch_add(1, Ordering::Relaxed)
    )
}
pub(crate) fn stale() -> String {
    "A proposta mudou desde a prévia; atualize e revise novamente.".into()
}
pub(crate) fn db_error(error: rusqlite::Error) -> String {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            "Banco de memória ocupado; tente novamente. Nenhuma alteração foi confirmada.".into()
        }
        _ => {
            "Não foi possível acessar o banco de memória; nenhuma alteração foi confirmada.".into()
        }
    }
}
