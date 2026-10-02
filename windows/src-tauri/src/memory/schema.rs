use rusqlite::{Connection, TransactionBehavior};

pub fn migrate(connection: &mut Connection) -> Result<(), String> {
    // Read the version after obtaining the write lock: two windows may open a
    // newly created database at once without both attempting the migration.
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(super::db_error)?;
    let version: i64 = tx
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(super::db_error)?;
    if version > 2 {
        return Err("O banco de memória foi criado por uma versão mais recente do Coucou.".into());
    }
    if version == 0 {
        tx.execute_batch(r#"
        CREATE TABLE preferences (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL);
        INSERT INTO preferences VALUES(1,'{"learningEnabled":true,"persistHistory":true,"autoSaveUserFacts":true}');
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, kind TEXT NOT NULL, memory_key TEXT NOT NULL, content TEXT NOT NULL,
            scope TEXT NOT NULL, source TEXT NOT NULL, confidence REAL NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('pending','approved','rejected','superseded')),
            revision INTEGER NOT NULL CHECK(revision>=1), replaces_id TEXT, base_revision INTEGER,
            created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX memory_active_key ON memories(kind,memory_key,scope) WHERE state='approved';
        CREATE INDEX memory_scope_state ON memories(scope,state,updated_at);
        CREATE VIRTUAL TABLE memory_fts USING fts5(memory_key,content,content='memories',content_rowid='rowid',tokenize='unicode61 remove_diacritics 2');
        CREATE TRIGGER memory_ai AFTER INSERT ON memories BEGIN INSERT INTO memory_fts(rowid,memory_key,content) VALUES(new.rowid,new.memory_key,new.content); END;
        CREATE TRIGGER memory_ad AFTER DELETE ON memories BEGIN INSERT INTO memory_fts(memory_fts,rowid,memory_key,content) VALUES('delete',old.rowid,old.memory_key,old.content); END;
        CREATE TRIGGER memory_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memory_fts(memory_fts,rowid,memory_key,content) VALUES('delete',old.rowid,old.memory_key,old.content);
            INSERT INTO memory_fts(rowid,memory_key,content) VALUES(new.rowid,new.memory_key,new.content);
        END;
        CREATE TABLE skills (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL, body TEXT NOT NULL,
            scope TEXT NOT NULL, source TEXT NOT NULL, ownership TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('pending','approved','rejected','superseded')),
            revision INTEGER NOT NULL CHECK(revision>=1), version INTEGER NOT NULL CHECK(version>=1),
            replaces_id TEXT, base_revision INTEGER, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX skill_active_name ON skills(name,scope) WHERE state='approved';
        CREATE INDEX skill_scope_state ON skills(scope,state,updated_at);
        CREATE VIRTUAL TABLE skill_fts USING fts5(name,description,body,content='skills',content_rowid='rowid',tokenize='unicode61 remove_diacritics 2');
        CREATE TRIGGER skill_ai AFTER INSERT ON skills BEGIN INSERT INTO skill_fts(rowid,name,description,body) VALUES(new.rowid,new.name,new.description,new.body); END;
        CREATE TRIGGER skill_ad AFTER DELETE ON skills BEGIN INSERT INTO skill_fts(skill_fts,rowid,name,description,body) VALUES('delete',old.rowid,old.name,old.description,old.body); END;
        CREATE TRIGGER skill_au AFTER UPDATE ON skills BEGIN
            INSERT INTO skill_fts(skill_fts,rowid,name,description,body) VALUES('delete',old.rowid,old.name,old.description,old.body);
            INSERT INTO skill_fts(rowid,name,description,body) VALUES(new.rowid,new.name,new.description,new.body);
        END;
        CREATE TABLE conversations (
            id TEXT PRIMARY KEY, agent TEXT NOT NULL, session_id TEXT, cwd TEXT, title TEXT NOT NULL,
            created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
        );
        CREATE TABLE messages (
            id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            role TEXT NOT NULL, content TEXT NOT NULL, created_at INTEGER NOT NULL
        );
        CREATE INDEX message_conversation_time ON messages(conversation_id,created_at);
        CREATE VIRTUAL TABLE message_fts USING fts5(content,content='messages',content_rowid='rowid',tokenize='unicode61 remove_diacritics 2');
        CREATE TRIGGER message_ai AFTER INSERT ON messages BEGIN INSERT INTO message_fts(rowid,content) VALUES(new.rowid,new.content); END;
        CREATE TRIGGER message_ad AFTER DELETE ON messages BEGIN INSERT INTO message_fts(message_fts,rowid,content) VALUES('delete',old.rowid,old.content); END;
        CREATE TRIGGER message_au AFTER UPDATE ON messages BEGIN
            INSERT INTO message_fts(message_fts,rowid,content) VALUES('delete',old.rowid,old.content);
            INSERT INTO message_fts(rowid,content) VALUES(new.rowid,new.content);
        END;
    "#).map_err(super::db_error)?;
        // FTS5 and modern SQLite are required, not silently replaced by a partial index.
        for table in ["memory_fts", "skill_fts", "message_fts"] {
            tx.execute(
                &format!("INSERT INTO {table}({table},rank) VALUES('secure-delete',1)"),
                [],
            )
            .map_err(super::db_error)?;
        }
    }
    if version < 2 {
        // A binding may be created before its first message. It carries no
        // native-session ownership and is never inferred from old history.
        tx.execute_batch(r#"
            CREATE TABLE personal_context_bindings (
                conversation_id TEXT PRIMARY KEY,
                context_id TEXT NOT NULL CHECK(context_id='personal-main'),
                created_at INTEGER NOT NULL
            );
            CREATE INDEX personal_context_members ON personal_context_bindings(context_id,conversation_id);
            PRAGMA user_version=2;
        "#).map_err(super::db_error)?;
    }
    // The human-authorized continuous personal conversation supersedes old
    // opt-in flags, without resetting memories, skills or transcripts. A
    // malformed preference record remains an error instead of being hidden.
    super::preferences_in(&tx)?;
    let preferences = serde_json::to_string(&super::MemoryPreferences::default())
        .map_err(|_| "Preferência de memória inválida.".to_string())?;
    tx.execute("UPDATE preferences SET json=?1 WHERE id=1", [preferences])
        .map_err(super::db_error)?;
    tx.commit().map_err(super::db_error)
}
