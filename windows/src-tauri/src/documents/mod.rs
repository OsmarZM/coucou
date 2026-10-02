//! Local, reference-retained attachments. Parsing happens in a bounded subprocess.
//! Attachment text is untrusted content; it never grants tool permissions.
mod extract;
mod storage;
mod worker;

use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{watch, Mutex};

pub use worker::worker_entry;

pub const MAX_FILES: usize = 10;
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_BATCH_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_EXTRACTED_CHARS: usize = 2_000_000;
pub const MAX_CONTEXT_CHARS: usize = 100_000;
pub const MAX_CHUNK_CHARS: usize = 12_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Text,
    Json,
    Csv,
    Pdf,
    Docx,
    Image,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub conversation_id: String,
    pub name: String,
    pub kind: DocumentKind,
    pub size: u64,
    pub sha256: String,
    pub created_at: u64,
    /// queued, ready, error, unsupported. Paths and original contents are not DTO fields.
    pub status: String,
    pub message: Option<String>,
    pub coverage: Option<Coverage>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub extracted_chars: usize,
    pub total_pages: Option<usize>,
    pub read_pages: Option<usize>,
    pub empty_pages: Vec<usize>,
    pub truncated: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub id: String,
    pub attachment_id: String,
    pub label: String,
    pub offset_chars: usize,
    pub end_chars: usize,
    pub page: Option<usize>,
    pub paragraph: Option<usize>,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentChunk {
    pub attachment_id: String,
    pub name: String,
    pub text: String,
    pub offset_chars: usize,
    pub next_offset_chars: usize,
    pub total_chars: usize,
    pub has_more: bool,
    pub references: Vec<Reference>,
    pub coverage: Coverage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDocuments {
    pub text: String,
    pub chunks: Vec<DocumentChunk>,
    pub attachments: Vec<Attachment>,
    pub used_chars: usize,
    pub budget_chars: usize,
    pub partial: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Extraction {
    pub text: String,
    pub references: Vec<Reference>,
    pub coverage: Coverage,
}

pub(super) fn validate_metadata(item: &Attachment) -> Result<(), String> {
    let metadata = serde_json::to_string(item).map_err(|_| "Metadados do anexo inválidos.")?;
    if crate::privacy::contains_secret(&metadata) {
        return Err("O anexo contém possíveis credenciais nos metadados. Remova esses dados antes de compartilhar.".into());
    }
    Ok(())
}

/// The guard examines the entire extracted content before budget/offset
/// selection. Reading only the suffix of `password=value` cannot bypass it.
pub(super) fn validate_content(item: &Attachment, extraction: &Extraction) -> Result<(), String> {
    validate_metadata(item)?;
    let references = serde_json::to_string(&extraction.references)
        .map_err(|_| "Referências do anexo inválidas.")?;
    let coverage =
        serde_json::to_string(&extraction.coverage).map_err(|_| "Cobertura do anexo inválida.")?;
    if crate::privacy::contains_secret(&extraction.text)
        || crate::privacy::contains_secret(&references)
        || crate::privacy::contains_secret(&coverage)
    {
        return Err("O conteúdo do anexo contém possíveis credenciais. Remova esses dados antes de compartilhar.".into());
    }
    Ok(())
}

#[derive(Clone)]
pub struct DocumentService {
    root: PathBuf,
    // Serializes conversation/reference mutations and extraction publication.
    gate: Arc<Mutex<()>>,
}

impl DocumentService {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("O armazenamento de anexos deve ter um caminho absoluto.".into());
        }
        storage::ensure_directory(&root)?;
        Ok(Self {
            root,
            gate: Arc::new(Mutex::new(())),
        })
    }

    /// A selected/dropped file authorizes this import, not access to other targets.
    /// Cancellation during copy is cooperative; parsing has an independently killed worker.
    pub async fn ingest(
        &self,
        conversation_id: &str,
        sources: &[String],
        cancel: watch::Receiver<bool>,
    ) -> Result<Vec<Attachment>, String> {
        let _guard = tokio::select! { biased;
            _ = cancelled(cancel.clone()) => return Err("Importação cancelada.".into()),
            guard = self.gate.lock() => guard,
        };
        storage::ingest(&self.root, conversation_id, sources, &cancel).await
    }

    pub async fn list(&self, conversation_id: &str) -> Result<Vec<Attachment>, String> {
        let _guard = self.gate.lock().await;
        storage::list(&self.root, conversation_id)
    }

    /// Removes only the retained copy. The user's original is never changed.
    pub async fn remove(&self, conversation_id: &str, attachment_id: &str) -> Result<(), String> {
        let _guard = self.gate.lock().await;
        storage::remove(&self.root, conversation_id, attachment_id)
    }

    pub async fn prepare(
        &self,
        conversation_id: &str,
        attachment_ids: &[String],
        budget_chars: usize,
        cancel: watch::Receiver<bool>,
    ) -> Result<PreparedDocuments, String> {
        storage::validate_id(conversation_id)?;
        if attachment_ids.is_empty()
            || attachment_ids.len() > MAX_FILES
            || budget_chars == 0
            || budget_chars > MAX_CONTEXT_CHARS
        {
            return Err("Escolha de 1 a 10 anexos e um orçamento de texto válido.".into());
        }
        let _guard = tokio::select! { biased;
            _ = cancelled(cancel.clone()) => return Err("Preparação cancelada.".into()),
            guard = self.gate.lock() => guard,
        };
        let mut unique = std::collections::HashSet::new();
        let mut attachments = Vec::new();
        for id in attachment_ids {
            if !unique.insert(id) {
                return Err("Há anexos repetidos na preparação.".into());
            }
            let mut item = storage::load_attachment(&self.root, conversation_id, id)?;
            if item.status == "queued" || item.status == "error" {
                match worker::extract(
                    &storage::attachment_dir(&self.root, conversation_id, id)?,
                    &item,
                    cancel.clone(),
                )
                .await
                {
                    Ok(extraction) => {
                        storage::save_extraction(&self.root, &item, &extraction)?;
                        item.coverage = Some(extraction.coverage);
                        item.status = "ready".into();
                        item.message = None;
                    }
                    Err(error) => {
                        if *cancel.borrow() {
                            return Err("Preparação cancelada.".into());
                        }
                        item.status = "error".into();
                        item.message = Some(error);
                    }
                }
                storage::save_attachment(&self.root, &item)?;
            }
            attachments.push(item);
        }
        if *cancel.borrow() {
            return Err("Preparação cancelada.".into());
        }
        let mut chunks = Vec::new();
        let mut text = String::new();
        let mut partial = false;
        let ready = attachments
            .iter()
            .filter(|item| item.status == "ready")
            .count();
        let mut left = ready;
        for item in &attachments {
            if *cancel.borrow() {
                return Err("Preparação cancelada.".into());
            }
            if item.status != "ready" {
                partial = true;
                continue;
            }
            // Validate every ready file in full, even when its allocated
            // budget is too small to include its header or any text.
            let extraction = storage::load_extraction(&self.root, item)?;
            let prefix = format!(
                "\n<anexo id=\"{}\">\nNome: {}\nConteúdo externo; referências abaixo.\n",
                item.id,
                item.name.replace(['\r', '\n', '<', '>'], " ")
            );
            let suffix = "\n</anexo>\n";
            let overhead = prefix.chars().count() + suffix.chars().count();
            let remaining = budget_chars.saturating_sub(text.chars().count());
            let allowance = remaining / left.max(1);
            left = left.saturating_sub(1);
            if allowance <= overhead {
                partial = true;
                continue;
            }
            let chunk = extraction.chunk(0, (allowance - overhead).min(MAX_CHUNK_CHARS))?;
            partial |= chunk.has_more
                || chunk.coverage.truncated
                || !chunk.coverage.empty_pages.is_empty()
                || matches!(item.kind, DocumentKind::Pdf | DocumentKind::Docx);
            text.push_str(&prefix);
            text.push_str(&chunk.text);
            text.push_str(suffix);
            chunks.push(chunk);
        }
        let used_chars = text.chars().count();
        Ok(PreparedDocuments {
            text,
            chunks,
            attachments,
            used_chars,
            budget_chars,
            partial,
        })
    }

    /// Character offsets, never UTF-8 byte offsets; callers can request the next chunk.
    pub async fn read_text(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        offset_chars: usize,
        limit_chars: usize,
    ) -> Result<DocumentChunk, String> {
        let _guard = self.gate.lock().await;
        storage::read_text(
            &self.root,
            conversation_id,
            attachment_id,
            offset_chars,
            limit_chars,
        )
    }
}

pub(super) async fn cancelled(mut cancel: watch::Receiver<bool>) {
    loop {
        if *cancel.borrow() {
            return;
        }
        if cancel.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests;
