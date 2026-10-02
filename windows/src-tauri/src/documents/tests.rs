use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("coucou-doc-tests-{}", storage::new_id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if self.0.starts_with(std::env::temp_dir()) && storage::reject_links(&self.0).is_ok() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn attachment(kind: DocumentKind, name: &str, bytes: &[u8]) -> Attachment {
    Attachment {
        id: "doc-test".into(),
        conversation_id: "conv-test".into(),
        name: name.into(),
        kind,
        size: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
        created_at: 1,
        status: "queued".into(),
        message: None,
        coverage: None,
    }
}

fn docx_bytes(xml: &str, extra: Option<(&str, &str)>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("[Content_Types].xml", options).unwrap();
    writer.write_all(b"<Types><Override ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\" PartName=\"/word/document.xml\"/></Types>").unwrap();
    writer.start_file("word/document.xml", options).unwrap();
    writer.write_all(xml.as_bytes()).unwrap();
    if let Some((name, contents)) = extra {
        writer.start_file(name, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

const WORD_XML: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>Olá &amp; ação &#x1F600;</w:t></w:r></w:p><w:p><w:r><w:t>Segundo</w:t><w:tab/><w:t>trecho</w:t></w:r></w:p></w:body></w:document>";

fn pdf_bytes(content: Option<&str>, page_count: usize) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let resources = document.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let mut kids = Vec::new();
    for _ in 0..page_count {
        let operations = content
            .map(|text| {
                vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![10.into(), 10.into()]),
                    Operation::new("Tj", vec![Object::string_literal(text)]),
                    Operation::new("ET", vec![]),
                ]
            })
            .unwrap_or_default();
        let stream = document.add_object(Stream::new(
            dictionary! {},
            Content { operations }.encode().unwrap(),
        ));
        let page = document.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => stream, "Resources" => resources, "MediaBox" => vec![0.into(),0.into(),300.into(),300.into()] });
        kids.push(Object::Reference(page));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => page_count as i64 },
        ),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn text_preserves_unicode_and_line_references() {
    let bytes = "ação 😀\nsegunda linha\n".as_bytes();
    let result =
        extract::extract_bytes(bytes, &attachment(DocumentKind::Text, "note.md", bytes)).unwrap();
    assert!(result.text.contains("ação 😀"));
    assert_eq!(result.references[0].line_start, Some(1));
    assert_eq!(result.references[0].line_end, Some(2));
    assert!(result.text.contains(&result.references[0].id));
    assert!(!result.coverage.truncated);
}

#[test]
fn utf16_bom_is_supported_but_binary_and_invalid_json_are_refused() {
    let bytes: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain("ação".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert!(
        extract::extract_bytes(&bytes, &attachment(DocumentKind::Text, "a.txt", &bytes))
            .unwrap()
            .text
            .contains("ação")
    );
    for bytes in [&b"abc\0xyz"[..], &b"\xff\x00"[..]] {
        assert!(
            extract::extract_bytes(bytes, &attachment(DocumentKind::Text, "a.txt", bytes)).is_err()
        );
    }
    let bytes = b"{bad json}";
    assert!(
        extract::extract_bytes(bytes, &attachment(DocumentKind::Json, "a.json", bytes)).is_err()
    );
}

#[test]
fn csv_multiline_cells_are_text_only_and_bad_quoting_is_refused() {
    let bytes = b"name,value\n\"two\nlines\",=SUM(A1)\n";
    let result =
        extract::extract_bytes(bytes, &attachment(DocumentKind::Csv, "a.csv", bytes)).unwrap();
    assert!(result.text.contains("=SUM(A1)"));
    assert!(!result.coverage.notes.is_empty());
    let bytes = b"a,\"not closed";
    assert!(extract::extract_bytes(bytes, &attachment(DocumentKind::Csv, "a.csv", bytes)).is_err());
}

#[test]
fn docx_has_paragraph_references_and_never_loads_external_links() {
    let bytes = docx_bytes(WORD_XML, Some(("word/_rels/document.xml.rels", "<Relationships><Relationship Target=\"https://example.invalid/secret\" TargetMode=\"External\"/></Relationships>")));
    let result =
        extract::extract_bytes(&bytes, &attachment(DocumentKind::Docx, "a.docx", &bytes)).unwrap();
    assert!(result.text.contains("Olá & ação 😀"));
    assert!(result.text.contains("Segundo\ttrecho"));
    assert_eq!(result.references[1].paragraph, Some(2));
    assert!(!result.text.contains("secret"));
}

#[test]
fn docx_refuses_macros_path_escape_dtd_and_invalid_xml() {
    for extra in [("word/vbaProject.bin", "macro"), ("../escape", "bad")] {
        let bytes = docx_bytes(WORD_XML, Some(extra));
        assert!(
            extract::extract_bytes(&bytes, &attachment(DocumentKind::Docx, "a.docx", &bytes))
                .is_err()
        );
    }
    for xml in ["<!DOCTYPE w:document [<!ENTITY x SYSTEM 'file:///C:/secret'>]><w:document/>", "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:t>hello</w:p>"] { assert!(extract::word_xml(xml, &attachment(DocumentKind::Docx, "a.docx", xml.as_bytes())).is_err()); }
    for xml in [
        format!("{WORD_XML}{WORD_XML}"),
        format!("{WORD_XML}trailing text"),
        WORD_XML.replace("<w:document", "<w:document broken"),
        WORD_XML
            .replace("<w:document", "<wrong:document xmlns:wrong=\"urn:invalid\"")
            .replace("</w:document>", "</wrong:document>"),
    ] {
        assert!(extract::word_xml(
            &xml,
            &attachment(DocumentKind::Docx, "a.docx", xml.as_bytes())
        )
        .is_err());
    }
}

#[tokio::test]
async fn secrets_are_checked_before_offsets_budgets_and_cache_publication() {
    let temp = Temporary::new();
    let source = temp.0.join("safe.txt");
    fs::write(&source, "harmless content").unwrap();
    let root = temp.0.join("documents");
    let service = DocumentService::new(root.clone()).unwrap();
    let (_sender, cancel) = watch::channel(false);
    let item = service
        .ingest("c", &[source.to_string_lossy().into_owned()], cancel)
        .await
        .unwrap()
        .remove(0);
    let raw = format!(
        "{}\npassword=DO_NOT_SHARE\nsuffix after the key",
        "safe ".repeat(2500)
    );
    let mut extraction = extract::extract_bytes(
        raw.as_bytes(),
        &attachment(DocumentKind::Text, "safe.txt", raw.as_bytes()),
    )
    .unwrap();
    for reference in &mut extraction.references {
        reference.attachment_id = item.id.clone();
    }
    assert!(storage::save_extraction(&root, &item, &extraction).is_err());
    // A preexisting/tampered cache still cannot expose only the secret suffix.
    let directory = storage::attachment_dir(&root, "c", &item.id).unwrap();
    fs::write(
        directory.join("text.json"),
        serde_json::to_vec(&extraction).unwrap(),
    )
    .unwrap();
    let mut ready = item.clone();
    ready.status = "ready".into();
    storage::save_attachment(&root, &ready).unwrap();
    let byte_offset = extraction.text.find("DO_NOT_SHARE").unwrap();
    let offset = extraction.text[..byte_offset].chars().count();
    let error = service
        .read_text("c", &item.id, offset, 12)
        .await
        .unwrap_err();
    assert!(!error.contains("DO_NOT_SHARE"));
    assert!(service.read_text("c", &item.id, 0, 10).await.is_err());
    let (_sender, cancel) = watch::channel(false);
    assert!(service
        .prepare("c", std::slice::from_ref(&item.id), 100, cancel)
        .await
        .is_err());
    let mut metadata = item;
    metadata.name = "password=DO_NOT_SHARE.txt".into();
    assert!(validate_metadata(&metadata).is_err());
    extraction.text = "safe".into();
    extraction.references[0].label = "password=DO_NOT_SHARE".into();
    assert!(validate_content(&ready, &extraction).is_err());
}

#[test]
fn pdf_text_is_referenced_by_page_and_scanned_or_too_many_pages_are_refused() {
    let bytes = pdf_bytes(Some("PDF fixture text"), 2);
    let result =
        extract::extract_bytes(&bytes, &attachment(DocumentKind::Pdf, "a.pdf", &bytes)).unwrap();
    assert!(result.text.contains("PDF fixture text"));
    assert_eq!(result.coverage.total_pages, Some(2));
    assert_eq!(result.references[1].page, Some(2));
    for bytes in [
        pdf_bytes(None, 1),
        pdf_bytes(Some("x"), 201),
        b"%PDF-invalid".to_vec(),
    ] {
        assert!(
            extract::extract_bytes(&bytes, &attachment(DocumentKind::Pdf, "a.pdf", &bytes))
                .is_err()
        );
    }
}

#[tokio::test]
async fn import_retains_reference_across_restart_duplicate_names_and_removal() {
    let temp = Temporary::new();
    let source = temp.0.join("a.txt");
    fs::write(&source, "original conteúdo").unwrap();
    let root = temp.0.join("documents");
    let service = DocumentService::new(root.clone()).unwrap();
    let (_tx, cancel) = watch::channel(false);
    let first = service
        .ingest(
            "conversation-a",
            &[source.to_string_lossy().into_owned()],
            cancel.clone(),
        )
        .await
        .unwrap()
        .remove(0);
    let second = service
        .ingest(
            "conversation-a",
            &[source.to_string_lossy().into_owned()],
            cancel,
        )
        .await
        .unwrap()
        .remove(0);
    assert_ne!(first.id, second.id);
    assert_eq!(first.sha256, second.sha256);
    let service = DocumentService::new(root).unwrap();
    assert_eq!(service.list("conversation-a").await.unwrap().len(), 2);
    assert!(service.remove("conversation-b", &first.id).await.is_err());
    service.remove("conversation-a", &first.id).await.unwrap();
    assert_eq!(fs::read_to_string(&source).unwrap(), "original conteúdo");
    assert_eq!(service.list("conversation-a").await.unwrap().len(), 1);
}

#[tokio::test]
async fn import_rejects_urls_escape_changed_format_large_file_and_cancel() {
    let temp = Temporary::new();
    let service = DocumentService::new(temp.0.join("documents")).unwrap();
    let (_tx, cancel) = watch::channel(false);
    assert!(service
        .ingest(
            "../bad",
            &["https://example.invalid/a.pdf".into()],
            cancel.clone()
        )
        .await
        .is_err());
    let fake = temp.0.join("fake.pdf");
    fs::write(&fake, "not a pdf").unwrap();
    assert!(service
        .ingest(
            "conv",
            &[fake.to_string_lossy().into_owned()],
            cancel.clone()
        )
        .await
        .is_err());
    assert!(service.list("conv").await.unwrap().is_empty());
    let big = temp.0.join("big.txt");
    fs::File::create(&big)
        .unwrap()
        .set_len(MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(service
        .ingest("conv", &[big.to_string_lossy().into_owned()], cancel)
        .await
        .is_err());
    let (_tx, cancel) = watch::channel(true);
    assert!(service
        .ingest(
            "conv",
            &[fake.to_string_lossy().into_owned()],
            cancel.clone()
        )
        .await
        .is_err());
    assert!(service
        .prepare("conv", &["x".into()], 1000, cancel)
        .await
        .is_err());
}

#[tokio::test]
async fn chunks_use_char_offsets_and_preparation_obeys_total_budget() {
    let temp = Temporary::new();
    let source = temp.0.join("a.txt");
    fs::write(&source, "ação 😀\n".repeat(2000)).unwrap();
    let root = temp.0.join("documents");
    let service = DocumentService::new(root.clone()).unwrap();
    let (_tx, cancel) = watch::channel(false);
    let mut item = service
        .ingest(
            "conv",
            &[source.to_string_lossy().into_owned()],
            cancel.clone(),
        )
        .await
        .unwrap()
        .remove(0);
    // Pure extraction fixture exercises the stored service without invoking the test binary as a worker.
    let extraction = extract::extract_file(
        &storage::attachment_dir(&root, "conv", &item.id)
            .unwrap()
            .join("original.bin"),
        &item,
    )
    .unwrap();
    storage::save_extraction(&root, &item, &extraction).unwrap();
    item.status = "ready".into();
    item.coverage = Some(extraction.coverage);
    storage::save_attachment(&root, &item).unwrap();
    let first = service.read_text("conv", &item.id, 0, 73).await.unwrap();
    let second = service
        .read_text("conv", &item.id, first.next_offset_chars, 91)
        .await
        .unwrap();
    assert_eq!(first.text.chars().count(), 73);
    assert_eq!(second.text.chars().count(), 91);
    assert!(first.has_more);
    let prepared = service
        .prepare("conv", &[item.id.clone()], 320, cancel)
        .await
        .unwrap();
    assert!(prepared.used_chars <= 320);
    assert_eq!(prepared.text.chars().count(), prepared.used_chars);
    assert!(prepared.partial);
    assert!(service.read_text("other", &item.id, 0, 10).await.is_err());
    assert!(service
        .read_text("conv", &item.id, usize::MAX, 10)
        .await
        .is_err());
}

#[test]
fn copied_file_hash_detects_later_modification() {
    let temp = Temporary::new();
    let path = temp.0.join("original.bin");
    fs::write(&path, b"safe").unwrap();
    let item = attachment(DocumentKind::Text, "a.txt", b"safe");
    assert!(extract::extract_file(Path::new(&path), &item).is_ok());
    fs::write(&path, b"evil").unwrap();
    assert!(extract::extract_file(&path, &item).is_err());
}

#[tokio::test]
async fn worker_cancel_and_timeout_terminate_a_live_parser_process() {
    use std::time::{Duration, Instant};
    let temp = Temporary::new();
    let item = attachment(DocumentKind::Text, "a.txt", b"text");
    let script = "process.stdin.resume(); setInterval(() => {}, 1000);";
    let (tx, cancel) = watch::channel(false);
    let started = Instant::now();
    let (result, ()) = tokio::join!(
        worker::fixture(&temp.0, &item, cancel, script, Duration::from_secs(10)),
        async {
            tokio::time::sleep(Duration::from_millis(150)).await;
            tx.send(true).unwrap();
        }
    );
    assert!(result.unwrap_err().contains("cancelada"));
    assert!(started.elapsed() < Duration::from_secs(5));
    let (_tx, cancel) = watch::channel(false);
    let started = Instant::now();
    let result = worker::fixture(&temp.0, &item, cancel, script, Duration::from_millis(150)).await;
    assert!(result.unwrap_err().contains("encerrada"));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn worker_refuses_oversized_and_malformed_results_without_echoing_stdout() {
    use std::time::Duration;
    let temp = Temporary::new();
    let item = attachment(DocumentKind::Text, "a.txt", b"text");
    for script in [
        "process.stdin.resume(); process.stdout.write('x'.repeat(17*1024*1024));",
        "process.stdin.resume(); process.stdout.write('SECRET_UNTRUSTED_OUTPUT');",
    ] {
        let (_tx, cancel) = watch::channel(false);
        let error = worker::fixture(&temp.0, &item, cancel, script, Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(!error.contains("SECRET_UNTRUSTED_OUTPUT"));
        assert!(error.contains("limite seguro") || error.contains("resultado inválido"));
    }
}
