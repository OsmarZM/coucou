use super::{guards::*, *};
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};

const SKILL_COLUMNS: &str = "id,name,description,body,scope,source,ownership,state,revision,version,replaces_id,base_revision,created_at,updated_at";

impl MemoryService {
    /// The database is the only writer. Updating a skill proposes another
    /// version, and never overwrites an existing SKILL.md on disk.
    pub fn upsert_skill_candidate(
        &self,
        mut candidate: SkillCandidate,
    ) -> Result<SkillRecord, String> {
        validate_skill(&candidate)?;
        candidate.description = candidate.description.trim().to_string();
        candidate.body = candidate.body.trim().to_string();
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        if candidate.source.kind != SourceKind::User && !preferences_in(&tx)?.learning_enabled {
            return Err(
                "Aprendizado pessoal desativado; procedimento automático não armazenado.".into(),
            );
        }
        let source =
            serde_json::to_string(&candidate.source).map_err(|_| "Origem inválida.".to_string())?;
        let timestamp = now();
        if let Some(id) = &candidate.id {
            let current = by_id(&tx, id)?.ok_or("Procedimento não encontrado.")?;
            if current.state != ReviewState::Pending
                || candidate.expected_revision != Some(current.revision)
            {
                return Err(stale());
            }
            if current.name != candidate.name
                || current.scope != candidate.scope
                || current.ownership != candidate.ownership
            {
                return Err("Nome, escopo e autoria não podem mudar durante a edição.".into());
            }
            tx.execute("UPDATE skills SET description=?1,body=?2,source=?3,revision=revision+1,updated_at=?4 WHERE id=?5", params![candidate.description,candidate.body,source,timestamp,id]).map_err(db_error)?;
            let updated = by_id(&tx, id)?.ok_or("Procedimento não encontrado após a edição.")?;
            tx.commit().map_err(db_error)?;
            return Ok(updated);
        }
        if candidate.expected_revision.is_some() {
            return Err("Uma revisão esperada exige o identificador da proposta.".into());
        }
        let active = active_skill(&tx, &candidate.name, &candidate.scope)?;
        if let Some(active) = &active {
            if active.description == candidate.description
                && active.body == candidate.body
                && active.ownership == candidate.ownership
            {
                return Ok(active.clone());
            }
        }
        protect_ownership(active.as_ref(), &candidate)?;
        let pending = tx.query_row(&format!("SELECT {SKILL_COLUMNS} FROM skills WHERE name=?1 AND scope=?2 AND description=?3 AND body=?4 AND ownership=?5 AND source=?6 AND state='pending' ORDER BY created_at DESC LIMIT 1"), params![candidate.name,candidate.scope,candidate.description,candidate.body,candidate.ownership.as_str(),source], map_skill).optional().map_err(db_error)?;
        if let Some(pending) = pending {
            if pending.replaces_id == active.as_ref().map(|row| row.id.clone())
                && pending.base_revision == active.as_ref().map(|row| row.revision)
            {
                return Ok(pending);
            }
        }
        ensure_capacity(&tx, "skills")?;
        let id = new_id("skill");
        let version = active.as_ref().map_or(1, |row| row.version + 1);
        tx.execute("INSERT INTO skills(id,name,description,body,scope,source,ownership,state,revision,version,replaces_id,base_revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'pending',1,?8,?9,?10,?11,?11)", params![id,candidate.name,candidate.description,candidate.body,candidate.scope,source,candidate.ownership.as_str(),version,active.as_ref().map(|row| &row.id),active.as_ref().map(|row| row.revision),timestamp]).map_err(db_error)?;
        let record = by_id(&tx, &id)?.ok_or("Procedimento não encontrado após a gravação.")?;
        tx.commit().map_err(db_error)?;
        Ok(record)
    }

    pub fn approve_skill(
        &self,
        id: &str,
        expected_revision: u64,
        explicit_user_confirmation: bool,
    ) -> Result<SkillRecord, String> {
        if !explicit_user_confirmation {
            return Err(
                "A aprovação do procedimento exige confirmação explícita do usuário.".into(),
            );
        }
        validate_id(id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let record = by_id(&tx, id)?.ok_or("Procedimento não encontrado.")?;
        if record.state != ReviewState::Pending || record.revision != expected_revision {
            return Err(stale());
        }
        validate_skill(&as_candidate(&record))?;
        let active = active_skill(&tx, &record.name, &record.scope)?;
        protect_ownership(active.as_ref(), &as_candidate(&record))?;
        check_base(&record, active.as_ref())?;
        let timestamp = now();
        if let Some(active) = active {
            tx.execute("UPDATE skills SET state='superseded',revision=revision+1,updated_at=?1 WHERE id=?2", params![timestamp,active.id]).map_err(db_error)?;
        }
        tx.execute(
            "UPDATE skills SET state='approved',revision=revision+1,updated_at=?1 WHERE id=?2",
            params![timestamp, id],
        )
        .map_err(db_error)?;
        let approved = by_id(&tx, id)?.ok_or("Procedimento não encontrado após a aprovação.")?;
        tx.commit().map_err(db_error)?;
        Ok(approved)
    }

    pub fn reject_skill(&self, id: &str, expected_revision: u64) -> Result<SkillRecord, String> {
        validate_id(id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let record = by_id(&tx, id)?.ok_or("Procedimento não encontrado.")?;
        if record.state != ReviewState::Pending || record.revision != expected_revision {
            return Err(stale());
        }
        tx.execute(
            "UPDATE skills SET state='rejected',revision=revision+1,updated_at=?1 WHERE id=?2",
            params![now(), id],
        )
        .map_err(db_error)?;
        let rejected = by_id(&tx, id)?.ok_or("Procedimento não encontrado após a rejeição.")?;
        tx.commit().map_err(db_error)?;
        Ok(rejected)
    }

    pub fn list_skills(&self, query: &MemoryQuery) -> Result<Vec<SkillRecord>, String> {
        if let Some(scope) = &query.scope {
            validate_scope(scope)?;
        }
        let expression = search_expression(&query.query)?;
        let connection = self.lock()?;
        let sql = if expression.is_empty() {
            format!("SELECT {SKILL_COLUMNS} FROM skills WHERE (?1 IS NULL OR scope=?1) AND (?2 IS NULL OR state=?2) ORDER BY updated_at DESC,id LIMIT ?3")
        } else {
            format!("SELECT {SKILL_COLUMNS} FROM skills WHERE rowid IN (SELECT rowid FROM skill_fts WHERE skill_fts MATCH ?4) AND (?1 IS NULL OR scope=?1) AND (?2 IS NULL OR state=?2) ORDER BY updated_at DESC,id LIMIT ?3")
        };
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        let state = query.state.map(ReviewState::as_str);
        let rows = if expression.is_empty() {
            statement.query_map(params![query.scope, state, limit(query.limit)], map_skill)
        } else {
            statement.query_map(
                params![query.scope, state, limit(query.limit), expression],
                map_skill,
            )
        }
        .map_err(db_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
    }

    /// For review only. Both full values and the revision are returned; no
    /// truncated or stale preview is accepted by the subsequent approval.
    pub fn skill_diff(&self, id: &str, expected_revision: u64) -> Result<SkillDiff, String> {
        validate_id(id)?;
        let connection = self.lock()?;
        let record = by_id(&connection, id)?.ok_or("Procedimento não encontrado.")?;
        if record.state != ReviewState::Pending || record.revision != expected_revision {
            return Err(stale());
        }
        let active = active_skill(&connection, &record.name, &record.scope)?;
        check_base(&record, active.as_ref())?;
        let before = active
            .as_ref()
            .map(portable)
            .transpose()?
            .unwrap_or_default();
        let after = portable(&record)?;
        let mut diff = format!("--- versão aprovada\n+++ proposta v{}\n", record.version);
        for line in before.lines() {
            diff.push('-');
            diff.push_str(line);
            diff.push('\n');
        }
        for line in after.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        Ok(SkillDiff {
            candidate_id: record.id,
            revision: record.revision,
            predecessor_id: record.replaces_id,
            base_revision: record.base_revision,
            ownership: record.ownership,
            before,
            after,
            diff,
        })
    }

    pub fn load_skill(&self, id: &str, expected_revision: u64) -> Result<SkillRecord, String> {
        validate_id(id)?;
        let connection = self.lock()?;
        let record = by_id(&connection, id)?.ok_or("Procedimento não encontrado.")?;
        if record.state != ReviewState::Approved || record.revision != expected_revision {
            return Err("Procedimento indisponível ou alterado; atualize o contexto.".into());
        }
        validate_skill(&as_candidate(&record))?;
        Ok(record)
    }

    /// Return a portable artifact for explicit user export. No filesystem
    /// writes, executable scripts or global CLI installation occur here.
    pub fn export_skill(&self, id: &str, expected_revision: u64) -> Result<SkillExport, String> {
        let record = self.load_skill(id, expected_revision)?;
        Ok(SkillExport {
            directory_name: record.name.clone(),
            filename: "SKILL.md".into(),
            content: portable(&record)?,
            version: record.version,
            ownership: record.ownership,
        })
    }

    /// Supports the portable name/description front matter emitted here and
    /// ordinary one-line YAML scalars. Complex YAML fails visibly rather than
    /// guessing its meaning. Import is always a reviewed candidate.
    pub fn import_skill(
        &self,
        markdown: &str,
        scope: &str,
        source: SourceRef,
    ) -> Result<SkillRecord, String> {
        validate_text(markdown, MAX_SKILL_BYTES + 2048, "Arquivo SKILL.md")?;
        if source.kind != SourceKind::Import && source.kind != SourceKind::User {
            return Err("Importação exige origem de arquivo ou ação explícita do usuário.".into());
        }
        let (name, description, body) = parse_portable(markdown)?;
        self.upsert_skill_candidate(SkillCandidate {
            id: None,
            expected_revision: None,
            name,
            description,
            body,
            scope: scope.into(),
            source,
            ownership: SkillOwnership::Imported,
        })
    }

    /// Previous versions remain available until explicit forgetting. Restoring
    /// creates a pending version and requires the usual full diff and approval.
    pub fn propose_skill_restore(
        &self,
        version_id: &str,
        source: SourceRef,
    ) -> Result<SkillRecord, String> {
        if source.kind != SourceKind::User {
            return Err("Restaurar uma versão exige ação explícita do usuário.".into());
        }
        validate_id(version_id)?;
        let old = {
            let connection = self.lock()?;
            by_id(&connection, version_id)?.ok_or("Versão do procedimento não encontrada.")?
        };
        if !matches!(old.state, ReviewState::Approved | ReviewState::Superseded) {
            return Err("Somente versões anteriormente aprovadas podem ser restauradas.".into());
        }
        let mut candidate = as_candidate(&old);
        candidate.id = None;
        candidate.expected_revision = None;
        candidate.source = source;
        self.upsert_skill_candidate(candidate)
    }

    pub fn forget_skill(&self, id: &str, expected_revision: u64) -> Result<usize, String> {
        validate_id(id)?;
        let mut connection = self.lock()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let record = by_id(&tx, id)?.ok_or("Procedimento não encontrado.")?;
        if record.revision != expected_revision {
            return Err(stale());
        }
        let deleted = tx
            .execute(
                "DELETE FROM skills WHERE name=?1 AND scope=?2",
                params![record.name, record.scope],
            )
            .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(deleted)
    }
}

fn validate_skill(candidate: &SkillCandidate) -> Result<(), String> {
    let name = &candidate.name;
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err(
            "Nome do procedimento deve ter até 64 caracteres minúsculos, números ou hífens.".into(),
        );
    }
    validate_scope(&candidate.scope)?;
    validate_text(&candidate.description, 1024, "Descrição")?;
    validate_text(&candidate.body, MAX_SKILL_BYTES, "Procedimento")?;
    validate_source(&candidate.source)?;
    if let Some(id) = &candidate.id {
        validate_id(id)?;
    }
    if candidate.ownership == SkillOwnership::User && candidate.source.kind != SourceKind::User {
        return Err("Uma proposta automática não pode reivindicar autoria do usuário.".into());
    }
    if candidate.ownership == SkillOwnership::Imported
        && !matches!(candidate.source.kind, SourceKind::Import | SourceKind::User)
    {
        return Err(
            "Autoria importada exige origem de arquivo ou edição explícita do usuário.".into(),
        );
    }
    Ok(())
}
fn protect_ownership(
    active: Option<&SkillRecord>,
    candidate: &SkillCandidate,
) -> Result<(), String> {
    if let Some(active) = active {
        if active.ownership != candidate.ownership {
            return Err("A autoria do procedimento existente deve ser preservada; use outro nome para criar uma cópia.".into());
        }
        if active.ownership != SkillOwnership::Coucou && candidate.source.kind != SourceKind::User {
            return Err("Procedimentos do usuário ou importados só podem ser alterados por proposta explícita do usuário.".into());
        }
    }
    Ok(())
}
fn check_base(record: &SkillRecord, active: Option<&SkillRecord>) -> Result<(), String> {
    if record.replaces_id != active.map(|row| row.id.clone())
        || record.base_revision != active.map(|row| row.revision)
    {
        return Err("A versão aprovada mudou; crie e revise outra proposta.".into());
    }
    Ok(())
}
fn as_candidate(record: &SkillRecord) -> SkillCandidate {
    SkillCandidate {
        id: Some(record.id.clone()),
        expected_revision: Some(record.revision),
        name: record.name.clone(),
        description: record.description.clone(),
        body: record.body.clone(),
        scope: record.scope.clone(),
        source: record.source.clone(),
        ownership: record.ownership,
    }
}
fn portable(record: &SkillRecord) -> Result<String, String> {
    validate_skill(&as_candidate(record))?;
    let name = serde_json::to_string(&record.name).map_err(|_| "Nome inválido.".to_string())?;
    let description = serde_json::to_string(&record.description)
        .map_err(|_| "Descrição inválida.".to_string())?;
    Ok(format!(
        "---\nname: {name}\ndescription: {description}\n---\n\n{}\n",
        record.body
    ))
}
fn parse_portable(markdown: &str) -> Result<(String, String, String), String> {
    let markdown = markdown
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n");
    let mut lines = markdown.lines();
    if lines.next() != Some("---") {
        return Err(
            "SKILL.md precisa iniciar com cabeçalho name/description entre linhas --- .".into(),
        );
    }
    let mut name = None;
    let mut description = None;
    let mut closed = false;
    let mut consumed = 4;
    for line in lines {
        consumed += line.len() + 1;
        if line == "---" {
            closed = true;
            break;
        }
        for (key, target) in [("name:", &mut name), ("description:", &mut description)] {
            if let Some(value) = line.strip_prefix(key) {
                if target.is_some() {
                    return Err("Campo duplicado no cabeçalho do procedimento.".into());
                }
                *target = Some(parse_scalar(value.trim())?);
            }
        }
    }
    if !closed {
        return Err("Cabeçalho do procedimento não foi fechado.".into());
    }
    let body = markdown.get(consumed..).unwrap_or("").trim().to_string();
    Ok((
        name.ok_or("Campo name ausente em SKILL.md.")?,
        description.ok_or("Campo description ausente em SKILL.md.")?,
        body,
    ))
}
fn parse_scalar(value: &str) -> Result<String, String> {
    if value.starts_with('"') {
        return serde_json::from_str::<String>(value)
            .map_err(|_| "Texto entre aspas inválido no cabeçalho.".into());
    }
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return Ok(value[1..value.len() - 1].replace("''", "'"));
    }
    if value.is_empty()
        || value.starts_with(['!', '&', '*', '>', '|', '[', '{'])
        || value.contains(" #")
    {
        return Err(
            "Use name e description como textos de uma linha; YAML complexo não é aceito.".into(),
        );
    }
    Ok(value.to_string())
}
fn active_skill(
    connection: &Connection,
    name: &str,
    scope: &str,
) -> Result<Option<SkillRecord>, String> {
    connection.query_row(&format!("SELECT {SKILL_COLUMNS} FROM skills WHERE name=?1 AND scope=?2 AND state='approved'"), params![name,scope], map_skill).optional().map_err(db_error)
}
fn by_id(connection: &Connection, id: &str) -> Result<Option<SkillRecord>, String> {
    connection
        .query_row(
            &format!("SELECT {SKILL_COLUMNS} FROM skills WHERE id=?1"),
            [id],
            map_skill,
        )
        .optional()
        .map_err(db_error)
}
fn map_skill(row: &Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        body: row.get(3)?,
        scope: row.get(4)?,
        source: serde_json::from_str(&row.get::<_, String>(5)?)
            .map_err(|_| conversion_error("Origem de procedimento inválida.".into()))?,
        ownership: SkillOwnership::parse(&row.get::<_, String>(6)?).map_err(conversion_error)?,
        state: ReviewState::parse(&row.get::<_, String>(7)?).map_err(conversion_error)?,
        revision: row.get(8)?,
        version: row.get(9)?,
        replaces_id: row.get(10)?,
        base_revision: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}
pub(crate) fn all(connection: &Connection) -> Result<Vec<SkillRecord>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {SKILL_COLUMNS} FROM skills ORDER BY created_at,id"
        ))
        .map_err(db_error)?;
    let result = statement
        .query_map([], map_skill)
        .map_err(db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(result)
}

pub(crate) fn append_context(
    connection: &Connection,
    expression: &str,
    scope: &str,
    maximum: usize,
    snapshot: &mut ContextSnapshot,
) -> Result<(), String> {
    let sql = if expression.is_empty() {
        format!("SELECT {SKILL_COLUMNS} FROM skills WHERE state='approved' AND scope IN ('user',?1) ORDER BY CASE WHEN scope=?1 THEN 0 ELSE 1 END,updated_at DESC LIMIT 8")
    } else {
        format!("SELECT {SKILL_COLUMNS} FROM skills WHERE state='approved' AND scope IN ('user',?1) AND rowid IN (SELECT rowid FROM skill_fts WHERE skill_fts MATCH ?2) ORDER BY CASE WHEN scope=?1 THEN 0 ELSE 1 END,updated_at DESC LIMIT 8")
    };
    let mut statement = connection.prepare(&sql).map_err(db_error)?;
    let rows = if expression.is_empty() {
        statement.query_map([scope], map_skill)
    } else {
        statement.query_map(params![scope, expression], map_skill)
    }
    .map_err(db_error)?;
    for row in rows {
        let record = row.map_err(db_error)?;
        validate_skill(&as_candidate(&record))?;
        // Discovery is progressive: a compact catalogue points to load_skill,
        // which returns the complete approved body for this exact revision.
        let line = format!(
            "- [procedimento {} / revisão {} / v{} / {} / origem {}] {}: {}\n",
            record.id,
            record.revision,
            record.version,
            record.scope,
            record.source.kind.as_str(),
            record.name,
            record.description
        );
        if snapshot.text.len() + line.len() > maximum {
            snapshot.truncated = true;
            continue;
        }
        snapshot.text.push_str(&line);
        snapshot.skills.push(ContextReference {
            id: record.id,
            revision: record.revision,
            scope: record.scope,
            source: record.source.kind,
        });
    }
    Ok(())
}
