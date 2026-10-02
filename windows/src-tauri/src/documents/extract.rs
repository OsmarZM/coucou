//! Pure parsers. The production service invokes these only in the resource-limited worker.
use super::*;
use quick_xml::{events::Event, Reader};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
};

const MAX_XML_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ZIP_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_PAGES: usize = 200;
const MAX_PAGE_DECOMPRESSED_BYTES: usize = 8 * 1024 * 1024;

struct Builder {
    item: Attachment,
    text: String,
    references: Vec<Reference>,
    coverage: Coverage,
    chars: usize,
}
impl Builder {
    fn new(item: &Attachment) -> Self {
        Self {
            item: item.clone(),
            text: String::new(),
            references: Vec::new(),
            coverage: Coverage::default(),
            chars: 0,
        }
    }
    fn push(
        &mut self,
        content: &str,
        label: String,
        page: Option<usize>,
        paragraph: Option<usize>,
        lines: Option<(usize, usize)>,
    ) {
        if self.references.len() >= 30_000 {
            self.coverage.truncated = true;
            return;
        }
        let id = format!("{}:r{}", self.item.id, self.references.len() + 1);
        let header = format!("[{id} — {label}]\n");
        let offset = self.chars;
        let available = MAX_EXTRACTED_CHARS.saturating_sub(self.chars);
        if available <= header.chars().count() + 1 {
            self.coverage.truncated = true;
            return;
        }
        self.text.push_str(&header);
        self.chars += header.chars().count();
        let wanted = content.chars().count();
        let allowance = MAX_EXTRACTED_CHARS.saturating_sub(self.chars + 1);
        let copied: String = content.chars().take(allowance).collect();
        self.chars += copied.chars().count() + 1;
        self.text.push_str(&copied);
        self.text.push('\n');
        self.coverage.truncated |= wanted > allowance;
        self.references.push(Reference {
            id,
            attachment_id: self.item.id.clone(),
            label,
            offset_chars: offset,
            end_chars: self.chars,
            page,
            paragraph,
            line_start: lines.map(|lines| lines.0),
            line_end: lines.map(|lines| lines.1),
        });
    }
    fn finish(mut self) -> Extraction {
        self.coverage.extracted_chars = self.chars;
        Extraction {
            text: self.text,
            references: self.references,
            coverage: self.coverage,
        }
    }
}

pub(super) fn extract_file(path: &Path, item: &Attachment) -> Result<Extraction, String> {
    let file = File::open(path).map_err(|_| "A cópia do anexo não está disponível.")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Não foi possível ler a cópia do anexo.")?;
    if bytes.len() as u64 != item.size
        || bytes.len() as u64 > MAX_FILE_BYTES
        || format!("{:x}", Sha256::digest(&bytes)) != item.sha256
    {
        return Err("A cópia do anexo foi alterada; anexe o original novamente.".into());
    }
    extract_bytes(&bytes, item)
}

pub(super) fn extract_bytes(bytes: &[u8], item: &Attachment) -> Result<Extraction, String> {
    match item.kind {
        DocumentKind::Pdf => pdf(bytes, item),
        DocumentKind::Docx => docx(bytes, item),
        DocumentKind::Image => {
            Err("OCR e visão nativa ainda não estão homologados para este anexo.".into())
        }
        _ => text(bytes, item),
    }
}

fn decode(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(b"\xff\xfe") || bytes.starts_with(b"\xfe\xff") {
        if !(bytes.len() - 2).is_multiple_of(2) {
            return Err("Texto UTF-16 incompleto ou inválido.".into());
        }
        let little = bytes.starts_with(b"\xff\xfe");
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|unit| {
                if little {
                    u16::from_le_bytes([unit[0], unit[1]])
                } else {
                    u16::from_be_bytes([unit[0], unit[1]])
                }
            })
            .collect();
        return String::from_utf16(&units).map_err(|_| "Texto UTF-16 inválido.".into());
    }
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        "Codificação não suportada. Salve o texto como UTF-8 ou UTF-16 com BOM.".into()
    })
}

fn text(bytes: &[u8], item: &Attachment) -> Result<Extraction, String> {
    let content = decode(bytes)?;
    if content
        .chars()
        .any(|c| c == '\0' || (c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{c}')))
    {
        return Err("O arquivo contém dados binários ou controles não suportados.".into());
    }
    if item.kind == DocumentKind::Json {
        let ext = Path::new(&item.name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "jsonl" | "ndjson") {
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                serde_json::from_str::<serde_json::Value>(line).map_err(|_| {
                    "O JSON por linha é inválido ou excede a profundidade suportada."
                })?;
            }
        } else {
            serde_json::from_str::<serde_json::Value>(&content)
                .map_err(|_| "O documento JSON é inválido ou excede a profundidade suportada.")?;
        }
    }
    if item.kind == DocumentKind::Csv {
        validate_csv(&content, &item.name)?;
    }
    if content.trim().is_empty() {
        return Err("O anexo não contém texto utilizável.".into());
    }
    let mut builder = Builder::new(item);
    let mut group = String::new();
    let mut start = 1;
    let mut count = 0;
    for (index, line) in content.lines().enumerate() {
        if builder.coverage.truncated {
            break;
        }
        group.push_str(line);
        group.push('\n');
        count += 1;
        if count == 100 || group.len() > 24_000 {
            builder.push(
                &group,
                format!("linhas {start}–{}", index + 1),
                None,
                None,
                Some((start, index + 1)),
            );
            group.clear();
            count = 0;
            start = index + 2;
        }
    }
    if !group.is_empty() {
        builder.push(
            &group,
            format!("linhas {start}–{}", start + count - 1),
            None,
            None,
            Some((start, start + count - 1)),
        );
    }
    if item.kind == DocumentKind::Csv {
        builder.coverage.notes.push("CSV/TSV preservado como texto, sem inferir tipos ou executar fórmulas. Truncamento representa somente uma amostra do arquivo.".into());
    }
    Ok(builder.finish())
}

fn validate_csv(content: &str, name: &str) -> Result<(), String> {
    let first = content.lines().next().unwrap_or("");
    let separator = if name.to_ascii_lowercase().ends_with(".tsv") {
        '\t'
    } else if first.matches(';').count() > first.matches(',').count() {
        ';'
    } else {
        ','
    };
    let mut chars = content.chars().peekable();
    let mut quoted = false;
    let mut field_start = true;
    let mut quote_closed = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    quoted = false;
                    quote_closed = true;
                }
            }
        } else if character == separator || matches!(character, '\r' | '\n') {
            field_start = true;
            quote_closed = false;
        } else if quote_closed {
            return Err(
                "CSV/TSV inválido: há caracteres após o fechamento de uma célula entre aspas."
                    .into(),
            );
        } else if character == '"' && field_start {
            quoted = true;
            field_start = false;
        } else {
            field_start = false;
        }
    }
    if quoted {
        return Err("CSV/TSV inválido: uma célula entre aspas não foi fechada.".into());
    }
    Ok(())
}

fn pdf(bytes: &[u8], item: &Attachment) -> Result<Extraction, String> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("O arquivo não é um PDF válido.".into());
    }
    let document = lopdf::Document::load_mem(bytes)
        .map_err(|_| "PDF inválido, protegido ou não suportado pelo extrator.")?;
    if document.is_encrypted() || document.was_encrypted() {
        return Err(
            "PDF protegido por senha não pode ser preparado. Exporte uma cópia sem proteção."
                .into(),
        );
    }
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err("O PDF não contém páginas válidas.".into());
    }
    if pages.len() > MAX_PDF_PAGES {
        return Err("O PDF excede 200 páginas. Divida-o antes de anexar.".into());
    }
    let mut builder = Builder::new(item);
    builder.coverage.total_pages = Some(pages.len());
    builder.coverage.read_pages = Some(0);
    for page in pages.keys().copied() {
        if builder.coverage.truncated {
            break;
        }
        let content = document
            .extract_text_with_limit(&[page], MAX_PAGE_DECOMPRESSED_BYTES)
            .map_err(|_| "Não foi possível extrair uma página do PDF dentro do limite seguro.")?;
        *builder.coverage.read_pages.as_mut().unwrap() += 1;
        if content.trim().is_empty() {
            builder.coverage.empty_pages.push(page as usize);
            continue;
        }
        builder.push(
            &content,
            format!("página {page}"),
            Some(page as usize),
            None,
            None,
        );
    }
    if builder.references.is_empty() {
        return Err("O PDF não contém texto extraível. Pode estar escaneado; OCR ainda não está disponível.".into());
    }
    if !builder.coverage.empty_pages.is_empty() {
        builder.coverage.notes.push("Há páginas sem texto extraível. Imagens, gráficos e partes escaneadas não foram interpretados; OCR não foi executado.".into());
    }
    builder.coverage.notes.push("Extração textual por página; disposição visual, tabelas, imagens e fontes incomuns podem não ser reproduzidas fielmente.".into());
    Ok(builder.finish())
}

fn archive_text(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<String, String> {
    let entry = archive
        .by_name(name)
        .map_err(|_| "DOCX inválido ou protegido: uma parte obrigatória não está disponível.")?;
    if entry.size() > MAX_XML_BYTES {
        return Err("Uma parte XML do DOCX excede 8 MiB.".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_XML_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Uma parte do DOCX é inválida ou protegida.")?;
    if bytes.len() as u64 > MAX_XML_BYTES {
        return Err("Uma parte XML do DOCX excede o limite seguro.".into());
    }
    decode(&bytes)
}

fn docx(bytes: &[u8], item: &Attachment) -> Result<Extraction, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "DOCX inválido ou protegido.")?;
    if archive.len() > 2048 {
        return Err("O DOCX contém entradas demais para extração segura.".into());
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| "O DOCX contém uma entrada inválida ou protegida.")?;
        if entry.enclosed_name().is_none()
            || entry.name().contains('\\')
            || entry.name().contains(':')
            || entry.name().contains('\0')
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("O DOCX contém caminhos ou links não permitidos.".into());
        }
        let lowered = entry.name().to_ascii_lowercase();
        if lowered.contains("vbaproject") || lowered.contains("vbasignature") {
            return Err("Documentos com macros não são aceitos.".into());
        }
        total = total.saturating_add(entry.size());
        if total > MAX_ZIP_TOTAL_BYTES {
            return Err("O DOCX excede o limite de 64 MiB descompactados.".into());
        }
    }
    let types = archive_text(&mut archive, "[Content_Types].xml")?;
    if types.to_ascii_lowercase().contains("macroenabled")
        || types.to_ascii_lowercase().contains("vbaproject")
    {
        return Err("Documentos com macros não são aceitos.".into());
    }
    if !types.contains(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    ) {
        return Err("O arquivo ZIP não é um documento DOCX de texto válido.".into());
    }
    let document = archive_text(&mut archive, "word/document.xml")?;
    let mut result = word_xml(&document, item)?;
    result.coverage.notes.push("Extraído o corpo principal em parágrafos; cabeçalhos, rodapés, notas, comentários, objetos embutidos e imagens não foram lidos. Links externos não foram acessados.".into());
    Ok(result)
}

pub(super) fn word_xml(xml: &str, item: &Attachment) -> Result<Extraction, String> {
    let mut reader = Reader::from_str(xml);
    let mut builder = Builder::new(item);
    let mut paragraph = String::new();
    let mut ordinal = 0;
    let mut depth = 0usize;
    let mut in_text = false;
    let mut root_seen = false;
    loop {
        match reader
            .read_event()
            .map_err(|_| "O XML do DOCX é inválido.")?
        {
            Event::Start(element) => {
                if depth == 0 && root_seen {
                    return Err("O XML do DOCX possui mais de um elemento raiz.".into());
                }
                depth += 1;
                if depth > 128 {
                    return Err("O DOCX excede a profundidade XML segura.".into());
                }
                let attributes = element
                    .attributes()
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "Os atributos XML do DOCX são inválidos.")?;
                let name = element.local_name();
                if !root_seen {
                    let namespace_key = element.name().into_inner().split_once(':').map_or_else(
                        || "xmlns".to_string(),
                        |(prefix, _)| format!("xmlns:{prefix}"),
                    );
                    if name.into_inner() != "document"
                        || !attributes.iter().any(|attribute| {
                            attribute.key.into_inner() == namespace_key.as_str()
                            && (attribute.value.as_ref()
                                == "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                                || attribute.value.as_ref()
                                    == "http://purl.oclc.org/ooxml/wordprocessingml/main")
                        })
                    {
                        return Err("O DOCX não possui um corpo WordprocessingML válido.".into());
                    }
                    root_seen = true;
                }
                match name.into_inner() {
                    "p" => {
                        ordinal += 1;
                        paragraph.clear();
                    }
                    "t" => in_text = true,
                    "tab" => paragraph.push('\t'),
                    "br" | "cr" => paragraph.push('\n'),
                    _ => {}
                }
            }
            Event::Empty(element) => {
                if depth == 0 {
                    return Err("O DOCX não possui um corpo XML de texto válido.".into());
                }
                for attribute in element.attributes() {
                    attribute.map_err(|_| "Os atributos XML do DOCX são inválidos.")?;
                }
                match element.local_name().into_inner() {
                    "tab" => paragraph.push('\t'),
                    "br" | "cr" => paragraph.push('\n'),
                    _ => {}
                }
            }
            Event::End(element) => {
                depth = depth.saturating_sub(1);
                match element.local_name().into_inner() {
                    "t" => in_text = false,
                    "p" if !paragraph.trim().is_empty() => {
                        builder.push(
                            &paragraph,
                            format!("parágrafo {ordinal}"),
                            None,
                            Some(ordinal),
                            None,
                        );
                    }
                    _ => {}
                }
            }
            Event::Text(value) if in_text => paragraph.push_str(&value.xml10_content()),
            Event::Text(value) if depth == 0 && !value.xml10_content().trim().is_empty() => {
                return Err("O XML do DOCX contém texto fora do elemento raiz.".into())
            }
            Event::CData(value) if in_text => paragraph.push_str(&value.xml10_content()),
            Event::GeneralRef(reference) if in_text => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|_| "Entidade XML inválida no DOCX.")?
                {
                    paragraph.push(character);
                } else if let Some(value) = quick_xml::escape::resolve_predefined_entity(&reference)
                {
                    paragraph.push_str(value);
                } else {
                    return Err("Entidades XML externas ou personalizadas não são aceitas.".into());
                }
            }
            Event::DocType(_) => {
                return Err("DTD e entidades externas não são aceitos em documentos.".into())
            }
            Event::Eof => break,
            _ => {}
        }
        if builder.coverage.truncated {
            break;
        }
    }
    if !root_seen || builder.references.is_empty() {
        return Err("O DOCX não contém texto utilizável no corpo principal.".into());
    }
    if !builder.coverage.truncated && depth != 0 {
        return Err("O XML do DOCX está incompleto.".into());
    }
    Ok(builder.finish())
}
