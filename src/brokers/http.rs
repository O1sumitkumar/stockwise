//! HTTP helpers shared by the broker clients.

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// HTTP client with the timeouts every broker call uses.
pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("stockwise-cli/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()?)
}

/// Error context for a failed order request: the order may or may not have
/// reached the exchange.
pub fn unknown_state(broker: &str) -> String {
    format!(
        "Its state is UNKNOWN — check the {broker} app or `stockwise trade orders` before retrying"
    )
}

/// Read a response body as text, keeping the HTTP status.
pub async fn body(resp: reqwest::Response) -> Result<(u16, String)> {
    let status = resp.status().as_u16();
    let text = resp.text().await.context("Could not read the response")?;
    Ok((status, text))
}

/// Parse `text` as JSON, naming the broker and request on failure. The body
/// excerpt is short and comes from the broker, never from our credentials.
pub fn parse<T: DeserializeOwned>(status: u16, text: &str, broker: &str, what: &str) -> Result<T> {
    serde_json::from_str(text).with_context(|| {
        format!(
            "Unexpected {broker} response to the {what} (HTTP {status}): {}",
            excerpt(text)
        )
    })
}

/// Parse a login response. On failure the body is withheld: it may hold a
/// token.
pub fn parse_login<T: DeserializeOwned>(status: u16, text: &str, broker: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|_| {
        anyhow::anyhow!(
            "Unexpected {broker} login response (HTTP {status}); body not shown because it may contain credentials"
        )
    })
}

/// First line of a response, capped, for error messages.
pub fn excerpt(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        out.push('…');
    }
    if out.is_empty() {
        "(empty body)".into()
    } else {
        out
    }
}

/// Fail with a clear message on HTTP 401/403: the saved session expired.
pub fn check_session(status: u16, broker: &str, id: &str) -> Result<()> {
    if status == 401 || status == 403 {
        bail!(
            "{broker} rejected the saved session (HTTP {status}). Log in again with \
             `stockwise trade login --broker {id}`."
        );
    }
    Ok(())
}

/// A JSON number or numeric string as f64 (brokers mix both).
pub fn num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().replace(',', "").parse().ok(),
        _ => None,
    }
    .filter(|x: &f64| x.is_finite())
}

/// A JSON string, number or bool as a String.
pub fn text(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A price that must be positive to mean anything (0 means "not available").
pub fn price(v: Option<&serde_json::Value>) -> Option<f64> {
    num(v).filter(|p| *p > 0.0)
}

/// Split one CSV line into fields (double quotes group, `""` escapes a quote).
pub fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches(['\r', '\n']).chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Gunzip `bytes` if they start with the gzip magic number, else return them.
pub fn maybe_gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut out)
            .context("Corrupt gzip data")?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Read one line from stdin after a prompt (for pasted redirect URLs).
pub fn prompt_line(prompt: &str) -> Result<String> {
    use std::io::{BufRead, Write};
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// Read a secret without echoing it when stdin is a terminal.
pub fn prompt_secret(prompt: &str) -> Result<String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        Ok(rpassword::prompt_password(prompt)?.trim().to_string())
    } else {
        prompt_line(prompt)
    }
}

/// Pull `key` out of a pasted redirect URL (`...?code=abc&state=x`), or
/// accept the bare value when no URL was pasted.
pub fn query_param(pasted: &str, key: &str) -> Option<String> {
    let pasted = pasted.trim();
    let Some((_, query)) = pasted.split_once('?') else {
        return (!pasted.is_empty() && !pasted.contains(['=', '&', '/', ' ']))
            .then(|| pasted.to_string());
    };
    let query = query.split('#').next().unwrap_or("");
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A random-enough OAuth `state` value.
pub fn oauth_state() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    format!("sw{:016x}", h.finish())
}

/// Lowercase hex SHA-256 of `input`.
pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_reads_redirect_urls_and_bare_values() {
        let url = "https://127.0.0.1/?action=login&type=login&status=success&request_token=abc123";
        assert_eq!(query_param(url, "request_token").as_deref(), Some("abc123"));
        assert_eq!(query_param(url, "status").as_deref(), Some("success"));
        assert_eq!(query_param(url, "missing"), None);
        assert_eq!(query_param("abc123", "code").as_deref(), Some("abc123"));
        assert_eq!(query_param("", "code"), None);
        assert_eq!(
            query_param("http://x/cb?code=a%2Fb&state=s#frag", "code").as_deref(),
            Some("a/b")
        );
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn num_accepts_numbers_and_strings() {
        use serde_json::json;
        assert_eq!(num(Some(&json!(1.5))), Some(1.5));
        assert_eq!(num(Some(&json!("1,234.50"))), Some(1234.5));
        assert_eq!(num(Some(&json!("x"))), None);
        assert_eq!(price(Some(&json!(0))), None);
        assert_eq!(text(Some(&json!(12))).as_deref(), Some("12"));
    }

    #[test]
    fn csv_fields_handle_quotes_and_crlf() {
        assert_eq!(csv_fields("a,\"b,c\",d\r"), vec!["a", "b,c", "d"]);
        assert_eq!(csv_fields("\"say \"\"hi\"\"\",x"), vec!["say \"hi\"", "x"]);
        assert_eq!(csv_fields("a,,"), vec!["a", "", ""]);
    }

    #[test]
    fn excerpt_is_short() {
        assert_eq!(excerpt(""), "(empty body)");
        assert!(excerpt(&"x".repeat(500)).chars().count() <= 161);
    }
}
