use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_METADATA_BYTES: u64 = 16 * 1024;
const MAX_EXTRACTION_BYTES: u64 = 16 * 1024 * 1024;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "d-{nanos:x}-{:x}-{:x}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

pub(super) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Identificador de conversa ou anexo inválido.".into());
    }
    let reserved = id.to_ascii_uppercase();
    if matches!(reserved.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && reserved.as_bytes()[3].is_ascii_digit())
    {
        return Err("Identificador reservado pelo Windows.".into());
    }
    Ok(())
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(super) fn reject_links(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err("Caminhos com '..' não são permitidos.".into());
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if linked(&meta) => return Err("Links, atalhos e pastas redirecionadas não podem ser anexados. Selecione o arquivo original.".into()),
            Ok(_) => {},
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => return Err("Não foi possível validar o caminho do anexo.".into()),
        }
    }
    Ok(())
}

pub(super) fn ensure_directory(path: &Path) -> Result<(), String> {
    reject_links(path)?;
    fs::create_dir_all(path).map_err(|_| "Não foi possível criar o armazenamento de anexos.")?;
    reject_links(path)
}

pub(super) fn attachment_dir(
    root: &Path,
    conversation_id: &str,
    id: &str,
) -> Result<PathBuf, String> {
    validate_id(conversation_id)?;
    validate_id(id)?;
    let path = root.join(conversation_id).join(id);
    reject_links(&path)?;
    Ok(path)
}

fn conversation_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let path = root.join(id);
    reject_links(&path)?;
    Ok(path)
}

fn checked_source(path: &str) -> Result<File, String> {
    if path.contains('\0')
        || path.contains("://")
        || path.starts_with("\\\\")
        || path.starts_with("//")
        || path.get(2..).is_some_and(|rest| rest.contains(':'))
    {
        return Err(
            "Anexe um arquivo local; URLs, rede e fluxos alternativos não são aceitos.".into(),
        );
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("O caminho do anexo deve ser absoluto.".into());
    }
    reject_links(path)?;
    let expected = path
        .canonicalize()
        .map_err(|_| "O arquivo selecionado não está disponível.")?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    let file = options.open(path).map_err(|_| {
        "Não foi possível abrir o anexo. Feche aplicações que estejam alterando o arquivo."
    })?;
    let metadata = file
        .metadata()
        .map_err(|_| "Não foi possível validar o arquivo aberto.")?;
    if !metadata.is_file() || linked(&metadata) {
        return Err("Selecione um arquivo comum, sem links ou diretórios.".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED},
        };
        let mut buffer = vec![0u16; 32768];
        let length = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(file.as_raw_handle()),
                &mut buffer,
                FILE_NAME_NORMALIZED,
            )
        } as usize;
        if length == 0
            || length >= buffer.len()
            || Path::new(&String::from_utf16_lossy(&buffer[..length])) != expected
        {
            return Err(
                "O destino do arquivo mudou durante a validação. Selecione-o novamente.".into(),
            );
        }
    }
    #[cfg(not(windows))]
    {
        if path.canonicalize().ok().as_ref() != Some(&expected) {
            return Err("O destino do arquivo mudou.".into());
        }
    }
    Ok(file)
}

fn kind(source: &Path) -> Result<DocumentKind, String> {
    let ext = source
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" | "jsonl" | "ndjson" => Ok(DocumentKind::Json),
        "csv" | "tsv" => Ok(DocumentKind::Csv),
        "pdf" => Ok(DocumentKind::Pdf),
        "docx" => Ok(DocumentKind::Docx),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => Ok(DocumentKind::Image),
        "txt" | "md" | "markdown" | "log" | "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs"
        | "py" | "java" | "c" | "h" | "cpp" | "hpp" | "cs" | "go" | "rb" | "php" | "sql" | "sh"
        | "ps1" | "bat" | "cmd" | "yaml" | "yml" | "toml" | "xml" | "html" | "css" | "scss"
        | "ini" | "cfg" | "env" | "svg" => Ok(DocumentKind::Text),
        _ => Err(
            "Formato não suportado. Use texto, código, JSON, CSV, PDF textual ou DOCX sem macros."
                .into(),
        ),
    }
}

fn validate_signature(kind: &DocumentKind, prefix: &[u8]) -> Result<(), String> {
    let image = prefix.starts_with(b"\x89PNG\r\n\x1a\n")
        || prefix.starts_with(b"\xff\xd8\xff")
        || prefix.starts_with(b"GIF87a")
        || prefix.starts_with(b"GIF89a")
        || prefix.starts_with(b"BM")
        || (prefix.starts_with(b"RIFF") && prefix.get(8..12) == Some(b"WEBP"));
    let valid = match kind {
        DocumentKind::Pdf => prefix.starts_with(b"%PDF-"),
        DocumentKind::Docx => prefix.starts_with(b"PK\x03\x04"),
        DocumentKind::Image => image,
        _ => {
            !image
                && !prefix.starts_with(b"MZ")
                && !prefix.starts_with(b"%PDF-")
                && !prefix.starts_with(b"PK\x03\x04")
                && !prefix.starts_with(b"\x7fELF")
        }
    };
    if !valid {
        return Err("O conteúdo do arquivo não corresponde ao formato informado.".into());
    }
    Ok(())
}

struct PendingDirectory(PathBuf, bool);
impl Drop for PendingDirectory {
    fn drop(&mut self) {
        // An internal ID directory only; never a user's source or a recursive root.
        if !self.1 && reject_links(&self.0).is_ok() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

pub(super) async fn ingest(
    root: &Path,
    conversation_id: &str,
    sources: &[String],
    cancel: &watch::Receiver<bool>,
) -> Result<Vec<Attachment>, String> {
    let conversation = conversation_dir(root, conversation_id)?;
    if sources.is_empty() || sources.len() > MAX_FILES {
        return Err("Anexe de 1 a 10 arquivos por lote.".into());
    }
    let existing = list(root, conversation_id)?;
    if existing.len() + sources.len() > MAX_FILES {
        return Err("Esta conversa já atingiu o limite de 10 anexos. Remova um anexo antes de adicionar outro.".into());
    }
    let mut batch_size = 0u64;
    let mut inputs = Vec::new();
    for source in sources {
        if *cancel.borrow() {
            return Err("Importação cancelada.".into());
        }
        let file = checked_source(source)?;
        let size = file
            .metadata()
            .map_err(|_| "Não foi possível validar o tamanho do anexo.")?
            .len();
        if size == 0 || size > MAX_FILE_BYTES {
            return Err("Cada anexo deve ter conteúdo e no máximo 20 MiB.".into());
        }
        batch_size = batch_size.saturating_add(size);
        if batch_size > MAX_BATCH_BYTES {
            return Err("O lote excede o limite de 50 MiB.".into());
        }
        let path = Path::new(source);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("O nome do anexo não é Unicode válido.")?
            .to_string();
        if name.chars().count() > 255 || name.chars().any(char::is_control) {
            return Err("O nome do anexo contém caracteres inválidos.".into());
        }
        inputs.push((file, size, name, kind(path)?));
    }
    ensure_directory(&conversation)?;
    let mut directories = Vec::new();
    let mut attachments = Vec::new();
    for (source, size, name, kind) in inputs {
        let id = new_id();
        let dir = attachment_dir(root, conversation_id, &id)?;
        fs::create_dir(&dir).map_err(|_| "Não foi possível reservar o anexo.")?;
        directories.push(PendingDirectory(dir.clone(), false));
        let mut input = tokio::fs::File::from_std(source);
        let destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("original.bin"))
            .map_err(|_| "Não foi possível criar a cópia do anexo.")?;
        let mut output = tokio::fs::File::from_std(destination);
        let mut hash = Sha256::new();
        let mut copied = 0u64;
        let mut prefix = Vec::new();
        let mut bytes = vec![0u8; 65536];
        loop {
            if *cancel.borrow() {
                return Err("Importação cancelada.".into());
            }
            let length = input
                .read(&mut bytes)
                .await
                .map_err(|_| "Não foi possível ler o arquivo selecionado.")?;
            if length == 0 {
                break;
            }
            copied += length as u64;
            if copied > MAX_FILE_BYTES || copied > size {
                return Err("O arquivo mudou de tamanho durante a cópia.".into());
            }
            if prefix.is_empty() {
                prefix.extend_from_slice(&bytes[..length.min(512)]);
            }
            hash.update(&bytes[..length]);
            output
                .write_all(&bytes[..length])
                .await
                .map_err(|_| "Não foi possível salvar a cópia do anexo.")?;
        }
        if copied != size {
            return Err("O arquivo mudou durante a importação. Selecione-o novamente.".into());
        }
        validate_signature(&kind, &prefix)?;
        output
            .sync_all()
            .await
            .map_err(|_| "Não foi possível confirmar a cópia do anexo.")?;
        drop(output);
        let unsupported = kind == DocumentKind::Image;
        let item = Attachment { id, conversation_id: conversation_id.into(), name, kind, size, sha256: format!("{:x}", hash.finalize()), created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(), status: if unsupported { "unsupported" } else { "queued" }.into(), message: unsupported.then(|| "Imagem preservada. OCR e visão nativa ainda não foram homologados neste agente; nenhum conteúdo visual será enviado como texto.".into()), coverage: None };
        save_attachment(root, &item)?;
        attachments.push(item);
    }
    if *cancel.borrow() {
        return Err("Importação cancelada.".into());
    }
    for directory in &mut directories {
        directory.1 = true;
    }
    Ok(attachments)
}

fn read_json<T: for<'a> Deserialize<'a>>(path: &Path, limit: u64) -> Result<T, String> {
    reject_links(path)?;
    let file = File::open(path).map_err(|_| "O registro do anexo não está disponível.")?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Não foi possível ler o registro do anexo.")?;
    if bytes.len() as u64 > limit {
        return Err("O registro do anexo excede o limite seguro.".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "O registro do anexo é inválido.".to_string())
}

pub(super) fn atomic_json<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    reject_links(path)?;
    let temporary = path.with_extension(format!("{}.tmp", new_id()));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "Não foi possível preparar o registro do anexo.")?;
    let result = (|| {
        let bytes = serde_json::to_vec(data).map_err(|_| "Registro do anexo inválido.")?;
        if bytes.len() as u64 > MAX_EXTRACTION_BYTES {
            return Err("O registro do anexo excede o limite seguro.".into());
        }
        output
            .write_all(&bytes)
            .and_then(|_| output.sync_all())
            .map_err(|_| "Não foi possível salvar o registro do anexo.")?;
        drop(output);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::{
                core::PCWSTR,
                Win32::Storage::FileSystem::{
                    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
                },
            };
            let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(source.as_ptr()),
                    PCWSTR(target.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|_| "Não foi possível publicar o registro do anexo.")?;
        }
        #[cfg(not(windows))]
        {
            fs::rename(&temporary, path)
                .map_err(|_| "Não foi possível publicar o registro do anexo.")?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn save_attachment(root: &Path, item: &Attachment) -> Result<(), String> {
    validate_metadata(item)?;
    atomic_json(
        &attachment_dir(root, &item.conversation_id, &item.id)?.join("metadata.json"),
        item,
    )
}

pub(super) fn load_attachment(
    root: &Path,
    conversation_id: &str,
    id: &str,
) -> Result<Attachment, String> {
    let item: Attachment = read_json(
        &attachment_dir(root, conversation_id, id)?.join("metadata.json"),
        MAX_METADATA_BYTES,
    )?;
    if item.id != id
        || item.conversation_id != conversation_id
        || item.size > MAX_FILE_BYTES
        || item.sha256.len() != 64
    {
        return Err("O registro do anexo não corresponde a esta conversa.".into());
    }
    validate_metadata(&item)?;
    Ok(item)
}

pub(super) fn list(root: &Path, conversation_id: &str) -> Result<Vec<Attachment>, String> {
    let dir = conversation_dir(root, conversation_id)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut attachments = Vec::new();
    for entry in fs::read_dir(dir)
        .map_err(|_| "Não foi possível listar os anexos.")?
        .take(MAX_FILES + 1)
    {
        let entry = entry.map_err(|_| "Não foi possível ler a lista de anexos.")?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| "Há um registro de anexo inválido.")?;
        attachments.push(load_attachment(root, conversation_id, &id)?);
    }
    if attachments.len() > MAX_FILES {
        return Err("A lista de anexos excede o limite da conversa.".into());
    }
    attachments.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    Ok(attachments)
}

pub(super) fn remove(root: &Path, conversation_id: &str, id: &str) -> Result<(), String> {
    load_attachment(root, conversation_id, id)?;
    let dir = attachment_dir(root, conversation_id, id)?;
    // Validate every descendant before a recursive removal; no reparse escape.
    for entry in fs::read_dir(&dir).map_err(|_| "Não foi possível verificar a cópia do anexo.")?
    {
        let path = entry.map_err(|_| "Registro de anexo inválido.")?.path();
        reject_links(&path)?;
        if !fs::symlink_metadata(&path)
            .map_err(|_| "Registro de anexo inválido.")?
            .is_file()
        {
            return Err("A cópia do anexo contém uma entrada inesperada.".into());
        }
    }
    fs::remove_dir_all(dir).map_err(|_| "Não foi possível remover a cópia do anexo.".to_string())
}

pub(super) fn save_extraction(
    root: &Path,
    item: &Attachment,
    extraction: &Extraction,
) -> Result<(), String> {
    validate_content(item, extraction)?;
    atomic_json(
        &attachment_dir(root, &item.conversation_id, &item.id)?.join("text.json"),
        extraction,
    )
}

pub(super) fn read_text(
    root: &Path,
    conversation_id: &str,
    id: &str,
    offset: usize,
    limit: usize,
) -> Result<DocumentChunk, String> {
    if limit == 0 || limit > MAX_CHUNK_CHARS {
        return Err("O trecho deve conter de 1 a 12000 caracteres.".into());
    }
    let item = load_attachment(root, conversation_id, id)?;
    load_extraction(root, &item)?.chunk(offset, limit)
}

/// Only this loader constructs a validated cache. Preparation uses it before
/// considering its text budget, so a skipped attachment cannot skip privacy.
pub(super) fn load_extraction(root: &Path, item: &Attachment) -> Result<LoadedExtraction, String> {
    if item.status != "ready" {
        return Err(item
            .message
            .clone()
            .unwrap_or_else(|| "Este anexo ainda não foi preparado.".into()));
    }
    let extraction: Extraction = read_json(
        &attachment_dir(root, &item.conversation_id, &item.id)?.join("text.json"),
        MAX_EXTRACTION_BYTES,
    )?;
    validate_content(item, &extraction)?;
    let total = extraction.text.chars().count();
    if total > MAX_EXTRACTED_CHARS {
        return Err("O conteúdo do anexo excede o limite seguro.".into());
    }
    Ok(LoadedExtraction {
        item: item.clone(),
        extraction,
        total,
    })
}

pub(super) struct LoadedExtraction {
    item: Attachment,
    extraction: Extraction,
    total: usize,
}

impl LoadedExtraction {
    pub(super) fn chunk(self, offset: usize, limit: usize) -> Result<DocumentChunk, String> {
        if limit == 0 || limit > MAX_CHUNK_CHARS {
            return Err("O trecho deve conter de 1 a 12000 caracteres.".into());
        }
        if offset > self.total {
            return Err("O deslocamento ou o conteúdo do trecho é inválido.".into());
        }
        let end = offset.saturating_add(limit).min(self.total);
        let text = self
            .extraction
            .text
            .chars()
            .skip(offset)
            .take(end - offset)
            .collect();
        let references = self
            .extraction
            .references
            .into_iter()
            .filter(|reference| reference.offset_chars < end && reference.end_chars > offset)
            .collect();
        Ok(DocumentChunk {
            attachment_id: self.item.id,
            name: self.item.name,
            text,
            offset_chars: offset,
            next_offset_chars: end,
            total_chars: self.total,
            has_more: end < self.total,
            references,
            coverage: self.extraction.coverage,
        })
    }
}
