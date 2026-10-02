use super::models::{MemoryCandidate, MemoryKind, MemoryRecord, SourceRef};

pub(crate) const MAX_MEMORY_BYTES: usize = 8192;
pub(crate) const MAX_SKILL_BYTES: usize = 32 * 1024;

pub fn contains_sensitive_content(value: &str) -> bool {
    crate::privacy::contains_secret(value)
}

pub(crate) fn validate_text(value: &str, max: usize, label: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > max
        || value.contains('\0')
        || value.contains('\u{1b}')
    {
        return Err(format!(
            "{label} vazio, inválido ou acima do limite de {max} bytes."
        ));
    }
    if contains_sensitive_content(value) {
        return Err("Conteúdo possivelmente sensível não pode ser salvo em memória, histórico ou procedimentos.".into());
    }
    Ok(())
}

pub(crate) fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("Identificador inválido.".into());
    }
    Ok(())
}
pub(crate) fn validate_scope(scope: &str) -> Result<(), String> {
    validate_id(scope)?;
    if scope != "user"
        && !scope
            .strip_prefix("project:")
            .is_some_and(|id| !id.trim().is_empty())
        && !scope
            .strip_prefix("conversation:")
            .is_some_and(|id| !id.trim().is_empty())
    {
        return Err("Escopo de memória inválido.".into());
    }
    if contains_sensitive_content(scope) {
        return Err("Escopo sensível não permitido.".into());
    }
    Ok(())
}
pub(crate) fn validate_source(source: &SourceRef) -> Result<(), String> {
    validate_text(&source.reference, 512, "Origem")?;
    validate_text(&source.evidence, 4096, "Evidência")
}
pub(crate) fn validate_candidate(candidate: &MemoryCandidate) -> Result<(), String> {
    validate_scope(&candidate.scope)?;
    validate_memory_content(&candidate.key, &candidate.content)?;
    validate_source(&candidate.source)?;
    if !candidate.confidence.is_finite() || !(0.0..=1.0).contains(&candidate.confidence) {
        return Err("Confiança deve estar entre 0 e 1.".into());
    }
    if let Some(id) = &candidate.id {
        validate_id(id)?;
    }
    Ok(())
}
pub(crate) fn validate_memory_content(key: &str, content: &str) -> Result<(), String> {
    validate_text(key, 256, "Chave")?;
    validate_text(content, MAX_MEMORY_BYTES, "Memória")?;
    // The field relationship is an assignment too. Validating fields alone
    // would accept {key:"password",content:"opaque-value"} and store a secret.
    if contains_sensitive_content(&format!("{key}={content}")) {
        return Err("Conteúdo possivelmente sensível não pode ser salvo em memória.".into());
    }
    Ok(())
}

pub(crate) fn search_expression(query: &str) -> Result<String, String> {
    if query.len() > 1024 {
        return Err("Busca acima do limite de 1024 bytes.".into());
    }
    let terms: Vec<_> = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .take(12)
        .map(|term| format!("\"{term}\"*"))
        .collect();
    Ok(terms.join(" OR "))
}
pub(crate) fn bounded(value: &str, bytes: usize) -> String {
    let mut end = value.len().min(bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn normalized_statement(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            ch if ch.is_alphanumeric() => ch,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A conservative automatic-memory gate, independent from action approvals.
/// Missing a useful fact leaves a pending candidate/history, never a grant.
pub(crate) fn automatic_content_is_safe(key: &str, content: &str, evidence: &str) -> bool {
    [key, content, evidence].iter().all(|value| {
        if value.chars().any(char::is_control) || contains_sensitive_content(value) {
            return false;
        }
        let normalized = normalized_statement(value);
        ![
            "permiss",
            "autoriz",
            "aprova",
            "aprov",
            "credencial",
            "senha",
            "password",
            "token",
            "secret",
            "chave",
            "comando",
            "command",
            "execut",
            "execution",
            "execucao",
            "rodar",
            "bypass",
            "sandbox",
            "chat",
            "mensag",
            "envi",
            "send",
            "delet",
            "apaga",
            "remov",
            "processo",
            "procedimento",
            "passo",
            "workflow",
            "automat",
            "agenda",
            "monitor",
            "grant",
            "allow",
            "deny",
            "instru",
            "instruction",
            "ignora",
            "ignore",
            "sem pedir",
            "sem confirmar",
            "sem confirmacao",
            "assuma",
            "nunca pergunte",
            "acesso",
            "access",
            "arquivo",
            "file",
            "pasta",
            "folder",
            "diretorio",
            "session",
            "sessao",
            "email",
            "whatsapp",
            "slack",
            "notific",
            "deploy",
            "publicar",
            "push",
            "merge",
            "sudo",
            "admin",
            "powershell",
            "cmd",
            "system",
            "sistema",
            "permit",
            "liber",
            "pode ",
            "possa",
            "conced",
            "consent",
            "approv",
            "confirm",
        ]
        .iter()
        .any(|term| normalized.contains(term))
    })
}

pub(crate) fn has_direct_low_risk_statement(kind: MemoryKind, value: &str) -> bool {
    direct_low_risk_body(kind, value).is_some()
}

fn direct_low_risk_body(kind: MemoryKind, value: &str) -> Option<String> {
    let normalized = normalized_statement(value);
    let statement = ["lembre que ", "lembre se de que "]
        .iter()
        .find_map(|prefix| normalized.strip_prefix(prefix))
        .unwrap_or(&normalized);
    let prefixes: &[&str] = match kind {
        MemoryKind::Preference => &[
            "prefiro ",
            "minha preferencia e ",
            "gosto de ",
            "responda em ",
            "quero ",
        ],
        MemoryKind::Fact => &["trabalho com ", "uso ", "sou "],
        MemoryKind::Procedure => return None,
    };
    let (prefix, body) = prefixes
        .iter()
        .find_map(|prefix| statement.strip_prefix(prefix).map(|body| (*prefix, body)))?;
    let safe = match kind {
        MemoryKind::Preference => presentation_value(body),
        MemoryKind::Fact if prefix == "sou " => matches!(
            body,
            "desenvolvedor"
                | "desenvolvedor backend"
                | "product owner"
                | "desenvolvedor e product owner"
        ),
        MemoryKind::Fact => {
            let terms: Vec<_> = body.split_whitespace().collect();
            !terms.is_empty()
                && terms.iter().any(|term| *term != "e")
                && terms.iter().all(|term| {
                    matches!(
                        *term,
                        "node"
                            | "js"
                            | "python"
                            | "prisma"
                            | "postgresql"
                            | "mysql"
                            | "docker"
                            | "api"
                            | "apis"
                            | "rest"
                            | "bitrix24"
                            | "supabase"
                            | "rust"
                            | "tauri"
                            | "typescript"
                            | "javascript"
                            | "git"
                            | "react"
                            | "angular"
                            | "e"
                    )
                })
        }
        MemoryKind::Procedure => false,
    };
    safe.then(|| body.to_string())
}

fn presentation_value(body: &str) -> bool {
    // A finite presentation vocabulary prevents procedural requests disguised
    // as preferences from becoming persistent, automatically curated data.
    // Negation/modality is retained in the stored value, never stripped there.
    let style = ["nao usar ", "nao ", "evitar "]
        .iter()
        .find_map(|prefix| body.strip_prefix(prefix))
        .unwrap_or(body);
    matches!(
        style,
        "respostas curtas"
            | "respostas curtas e diretas"
            | "respostas diretas e curtas"
            | "respostas concisas"
            | "respostas detalhadas"
            | "respostas completas"
            | "respostas objetivas"
            | "respostas tecnicas"
            | "respostas tecnicas e objetivas"
            | "respostas diretas"
            | "respostas em portugues"
            | "respostas em portugues brasileiro"
            | "portugues"
            | "portugues brasileiro"
            | "pt br"
            | "tabelas"
            | "exemplos"
            | "exemplos praticos"
            | "exemplos de codigo"
            | "markdown"
            | "json"
            | "texto simples"
    )
}

pub(crate) fn canonical_automatic_key(kind: MemoryKind, statement: &str) -> Option<&'static str> {
    let body = direct_low_risk_body(kind, statement)?;
    match kind {
        MemoryKind::Preference => {
            let style = ["nao usar ", "nao ", "evitar "]
                .iter()
                .find_map(|prefix| body.strip_prefix(prefix))
                .unwrap_or(&body);
            Some(if style.contains("portugues") || style == "pt br" {
                "idioma"
            } else if style.starts_with("respostas ") {
                "estilo de resposta"
            } else if style.starts_with("exemplos") {
                "exemplos"
            } else {
                "formato de resposta"
            })
        }
        MemoryKind::Fact => Some(
            if matches!(
                body.as_str(),
                "desenvolvedor"
                    | "desenvolvedor backend"
                    | "product owner"
                    | "desenvolvedor e product owner"
            ) {
                "profissao"
            } else {
                "tecnologias"
            },
        ),
        MemoryKind::Procedure => None,
    }
}

fn supported_observation(kind: MemoryKind, key: &str, content: &str, evidence: &str) -> bool {
    if kind == MemoryKind::Procedure
        || evidence.len() > 1024
        || !automatic_content_is_safe(key, content, evidence)
        || !has_direct_low_risk_statement(kind, evidence)
        || evidence.contains(['`', '<', '>', '[', ']'])
        || !observed_key_matches(kind, key, evidence)
    {
        return false;
    }
    // Preserve the ENTIRE declaration/value, including its modality/negation.
    // A partial substring could turn evitar respostas curtas into its inverse.
    let content = normalized_statement(content);
    let original = normalized_statement(evidence);
    let body = direct_low_risk_body(kind, evidence);
    content.len() >= 3 && (content == original || body.as_deref() == Some(content.as_str()))
}
fn observed_key_matches(kind: MemoryKind, key: &str, evidence: &str) -> bool {
    // The model also chooses a key. A literal value must not be relabeled as
    // permission, another person's identity, or unrelated profile metadata.
    let key = normalized_statement(key);
    match kind {
        MemoryKind::Preference => matches!(
            key.as_str(),
            "formato"
                | "formato de resposta"
                | "formato de respostas"
                | "apresentacao"
                | "preferencia de apresentacao"
                | "estilo de resposta"
                | "idioma"
                | "exemplos"
        ),
        MemoryKind::Fact if normalized_statement(evidence).starts_with("sou ") => {
            matches!(key.as_str(), "profissao" | "funcao" | "ocupacao")
        }
        MemoryKind::Fact => matches!(key.as_str(), "tecnologia" | "tecnologias" | "stack"),
        MemoryKind::Procedure => false,
    }
}
pub(crate) fn observed_statement_is_supported(candidate: &MemoryCandidate) -> bool {
    supported_observation(
        candidate.kind,
        &candidate.key,
        &candidate.content,
        &candidate.source.evidence,
    )
}
pub(crate) fn observed_record_is_supported(record: &MemoryRecord) -> bool {
    supported_observation(
        record.kind,
        &record.key,
        &record.content,
        &record.source.evidence,
    )
}
