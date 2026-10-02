//! Content protection without suppressing ordinary URLs, paths or assignments.
pub fn contains_secret(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return true;
    }
    if [
        "authorization: bearer",
        "authorization: basic",
        "cookie:",
        "set-cookie:",
        "senha é ",
        "password is ",
        "api key is ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }
    for word in text.split(|ch: char| {
        ch.is_whitespace() || matches!(ch, '\'' | '"' | '<' | '>' | '(' | ')' | ',' | ';')
    }) {
        if [
            "sk-",
            "sk-ant-",
            "ghp_",
            "gho_",
            "ghs_",
            "github_pat_",
            "xoxb-",
            "xoxp-",
            "gsk_",
            "hf_",
            "AIza",
            "ya29.",
        ]
        .iter()
        .any(|prefix| word.starts_with(prefix) && word.len() >= 24)
        {
            return true;
        }
        let jwt: Vec<_> = word.split('.').collect();
        if jwt.len() == 3
            && jwt[0].starts_with("eyJ")
            && jwt.iter().all(|part| {
                part.len() >= 8
                    && part
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            })
        {
            return true;
        }
        if let Some(scheme) = word.find("://") {
            let authority = word[scheme + 3..].split('/').next().unwrap_or("");
            if authority.contains('@')
                && authority
                    .split('@')
                    .next()
                    .is_some_and(|user| user.contains(':'))
            {
                return true;
            }
        }
    }
    // Match assignments anywhere, including quoted JSON and camelCase keys.
    // Ordinary URLs, paths, formulas and explicit placeholders remain visible.
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    for (index, (_, character)) in chars.iter().enumerate() {
        if !matches!(character, '=' | ':') {
            continue;
        }
        let mut end = index;
        while end > 0
            && (chars[end - 1].1.is_whitespace() || matches!(chars[end - 1].1, '\'' | '"'))
        {
            end -= 1;
        }
        let mut begin = end;
        while begin > 0
            && (chars[begin - 1].1.is_ascii_alphanumeric()
                || matches!(chars[begin - 1].1, '_' | '-'))
        {
            begin -= 1;
        }
        let key: String = chars[begin..end].iter().map(|(_, ch)| ch).collect();
        if ![
            "api_key",
            "apikey",
            "api-key",
            "password",
            "passwd",
            "senha",
            "secret",
            "secret_key",
            "secretkey",
            "private_key",
            "privatekey",
            "client_secret",
            "clientsecret",
            "token",
            "access_token",
            "accesstoken",
            "refresh_token",
            "refreshtoken",
            "database_url",
            "databaseurl",
            "connection_string",
            "connectionstring",
            "aws_secret_access_key",
            "authorization",
        ]
        .contains(&key.as_str())
            && !key.ends_with("_api_key")
            && !key.ends_with("_token")
        {
            continue;
        }
        // Borrow the suffix by UTF-8 byte position. Copying the whole suffix
        // for each redacted assignment would make long documents quadratic.
        let rest = &lower[chars[index].0 + 1..];
        let value =
            rest.trim_start_matches(|ch: char| ch.is_whitespace() || matches!(ch, '\'' | '"'));
        if value.is_empty() {
            continue;
        }
        if key == "authorization" && !value.starts_with("bearer ") && !value.starts_with("basic ") {
            continue;
        }
        if [
            "<redacted>",
            "[redacted]",
            "***",
            "${",
            "<seu_",
            "<your_",
            "<token>",
            "<senha>",
        ]
        .iter()
        .any(|placeholder| value.starts_with(placeholder))
        {
            continue;
        }
        return true;
    }
    false
}
pub fn diagnostic(text: &str) -> String {
    if contains_secret(text) {
        return "Detalhes ocultados por proteção de dados.".into();
    }
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(2000)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_harmless_assignments_and_urls_but_blocks_credentials() {
        assert!(!contains_secret("files=42 https://docs.example.com"));
        assert!(contains_secret("api_key=private"));
        assert!(contains_secret("Authorization: Bearer private"));
        assert!(contains_secret("-----BEGIN PRIVATE KEY-----"));
        assert!(!diagnostic("files=42").contains("ocultados"));
    }
    #[test]
    fn catches_json_and_embedded_credentials_without_hiding_paths() {
        assert!(contains_secret(r#"{"clientSecret":"private-value"}"#));
        assert!(contains_secret(
            r#"{"Authorization":"Bearer private-value"}"#
        ));
        assert!(contains_secret("Authorization:Bearer private-value"));
        assert!(contains_secret("api_key = private-value"));
        assert!(contains_secret(
            "Veja https://user:private@host.example/path"
        ));
        assert!(!contains_secret(r"C:\Users\Example\Downloads\notes.md"));
        assert!(!contains_secret("https://docs.example.com/api?files=42"));
        assert!(!contains_secret(
            "O tipo de cabeçalho é Bearer e será configurado pelo usuário."
        ));
        assert!(!contains_secret("token=<redacted> API_KEY=${API_KEY}"));
        assert!(!contains_secret(&"token=<redacted>\n".repeat(4096)));
        assert!(
            !diagnostic(r"Falha ao abrir C:\Users\Example\Downloads\notes.md")
                .contains("ocultados")
        );
    }
}
