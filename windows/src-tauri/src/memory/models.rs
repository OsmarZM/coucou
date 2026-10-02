use serde::{Deserialize, Serialize};

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub enum $name { $($variant),+ }
        impl $name {
            pub(crate) fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } }
            pub(crate) fn parse(value: &str) -> Result<Self, String> {
                match value { $($value => Ok(Self::$variant)),+, _ => Err("Registro de memória incompatível.".into()) }
            }
        }
    };
}

text_enum!(MemoryKind { Preference => "preference", Fact => "fact", Procedure => "procedure" });
// Sources live in serialized SourceRef JSON, so they do not need the SQL text
// parser used by enums stored in individual database columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    User,
    Inference,
    Document,
    Import,
}
impl SourceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Inference => "inference",
            Self::Document => "document",
            Self::Import => "import",
        }
    }
}
text_enum!(ReviewState { Pending => "pending", Approved => "approved", Rejected => "rejected", Superseded => "superseded" });
text_enum!(SkillOwnership { User => "user", Coucou => "coucou", Imported => "imported" });
text_enum!(MessageRole { User => "user", Assistant => "assistant", Tool => "tool" });

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryPreferences {
    pub learning_enabled: bool,
    pub persist_history: bool,
    pub auto_save_user_facts: bool,
}
impl Default for MemoryPreferences {
    fn default() -> Self {
        Self {
            learning_enabled: true,
            persist_history: true,
            auto_save_user_facts: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRef {
    pub kind: SourceKind,
    /// A conversation/message or attachment identifier, never a credential.
    pub reference: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryCandidate {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    pub kind: MemoryKind,
    pub key: String,
    pub content: String,
    /// `user`, `project:<id>` or `conversation:<id>`.
    pub scope: String,
    pub source: SourceRef,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: String,
    pub kind: MemoryKind,
    pub key: String,
    pub content: String,
    pub scope: String,
    pub source: SourceRef,
    pub confidence: f64,
    pub state: ReviewState,
    pub revision: u64,
    pub replaces_id: Option<String>,
    pub base_revision: Option<u64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub state: Option<ReviewState>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            scope: None,
            state: None,
            limit: default_limit(),
        }
    }
}
pub(crate) fn default_limit() -> usize {
    100
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillCandidate {
    /// Updating an existing pending candidate requires its current revision.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    pub name: String,
    pub description: String,
    /// Markdown body; portable YAML front matter is generated on export.
    pub body: String,
    pub scope: String,
    pub source: SourceRef,
    pub ownership: SkillOwnership,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    pub scope: String,
    pub source: SourceRef,
    pub ownership: SkillOwnership,
    pub state: ReviewState,
    pub revision: u64,
    pub version: u64,
    pub replaces_id: Option<String>,
    pub base_revision: Option<u64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDiff {
    pub candidate_id: String,
    pub revision: u64,
    pub predecessor_id: Option<String>,
    pub base_revision: Option<u64>,
    pub ownership: SkillOwnership,
    pub before: String,
    pub after: String,
    /// Full bounded before/after values are included; a diff never hides omitted lines.
    pub diff: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExport {
    pub directory_name: String,
    pub filename: String,
    pub content: String,
    pub version: u64,
    pub ownership: SkillOwnership,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationMessage {
    pub id: String,
    pub conversation_id: String,
    pub agent: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub role: MessageRole,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub conversation_id: String,
    pub agent: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub title: String,
    pub message_count: u64,
    pub updated_at: i64,
    pub context_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalContextMessage {
    pub id: String,
    pub conversation_id: String,
    pub agent: String,
    pub role: MessageRole,
    pub content: String,
    /// Unix milliseconds, consistent with the existing local history.
    pub created_at: i64,
    pub content_truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalContextHistory {
    pub context_id: String,
    /// Informational JSON data. Neither messages nor bindings confer authority.
    pub text: String,
    pub messages: Vec<PersonalContextMessage>,
    /// Bytes of text plus serialized messages; metadata is separately bounded.
    pub bytes: usize,
    pub budget: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchHit {
    pub session: SessionRecord,
    pub message_id: String,
    pub excerpt: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryWrite {
    pub persisted: bool,
    pub duplicate: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextReference {
    pub id: String,
    pub revision: u64,
    pub scope: String,
    pub source: SourceKind,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSnapshot {
    pub text: String,
    pub memories: Vec<ContextReference>,
    pub skills: Vec<ContextReference>,
    pub bytes: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreExport {
    pub format_version: u32,
    pub exported_at: i64,
    pub preferences: MemoryPreferences,
    pub memories: Vec<MemoryRecord>,
    pub skills: Vec<SkillRecord>,
    pub sessions: Vec<SessionRecord>,
    pub messages: Vec<StoredMessage>,
}
