//! Disk cache for pages read from public websites (CCIL, RBI, NSE): one
//! file per page under `cache/` in the data directory.
//!
//! A fresh copy is used without a request; an old one is refreshed; and
//! when the site cannot be reached or refuses, a stale copy is still shown
//! with a warning and its date — never invented data.

use crate::store;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// A cached page and when it was downloaded.
#[derive(Debug, Clone)]
pub struct Page {
    pub text: String,
    pub fetched: DateTime<Utc>,
    /// Older than the caller's maximum age (the site could not be reached).
    pub stale: bool,
}

fn path(name: &str) -> Result<PathBuf> {
    let dir = store::data_dir()?.join("cache");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Cannot create cache directory {}", dir.display()))?;
    Ok(dir.join(name))
}

/// The cached copy of `name`, however old, if it passes `validate`.
pub fn read(name: &str, max_age: Duration, validate: impl Fn(&str) -> bool) -> Option<Page> {
    let path = path(name).ok()?;
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    if !validate(&text) {
        return None;
    }
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    Some(Page {
        text,
        fetched: modified.into(),
        stale: age >= max_age,
    })
}

/// Save `text` as `name` (temp file + rename). Best-effort: a read-only data
/// directory still works, just without a cache.
pub fn write(name: &str, text: &str) {
    let Ok(path) = path(name) else { return };
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// HTTP client for public data pages: a short timeout, a StockWise user agent.
pub fn client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!(
            "stockwise/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/iamsaquib8/stockwise)"
        ))
        .connect_timeout(Duration::from_secs(10).min(timeout))
        .timeout(timeout)
        .build()
        .context("Failed to build HTTP client")
}

/// GET `url` as text: one retry for network errors and 5xx, none for
/// refusals (403/429), so a blocked site is not hammered.
async fn get(client: &reqwest::Client, url: &str, source: &str) -> Result<String> {
    let mut last = None;
    for attempt in 0..2u64 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp
                    .text()
                    .await
                    .with_context(|| format!("Failed to read the {source} response"));
            }
            Ok(resp) if resp.status().is_server_error() => {
                last = Some(anyhow::anyhow!("{source} returned HTTP {}", resp.status()));
            }
            Ok(resp) => bail!("{source} refused the request (HTTP {})", resp.status()),
            Err(e) => last = Some(anyhow::anyhow!("Cannot reach {source} ({e})")),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("Cannot reach {source}")))
}

/// `url` from the cache when younger than `max_age`, else downloaded (and
/// cached). If the download fails, a stale copy is returned with a warning
/// on stderr; with no copy at all the error is returned.
pub async fn fetch(
    client: &reqwest::Client,
    name: &str,
    url: &str,
    source: &str,
    max_age: Duration,
    validate: impl Fn(&str) -> bool,
) -> Result<Page> {
    let cached = read(name, max_age, &validate);
    if let Some(page) = &cached
        && !page.stale
    {
        return Ok(page.clone());
    }
    match get(client, url, source).await {
        Ok(text) if validate(&text) => {
            write(name, &text);
            Ok(Page {
                text,
                fetched: Utc::now(),
                stale: false,
            })
        }
        Ok(_) => fall_back(
            cached,
            anyhow::anyhow!("{source} returned an unexpected page"),
        ),
        Err(e) => fall_back(cached, e),
    }
}

/// Use a stale copy (saying so on stderr), or return the error.
pub fn fall_back(cached: Option<Page>, err: anyhow::Error) -> Result<Page> {
    match cached {
        Some(page) => {
            eprintln!(
                "  Warning: {err:#}; showing the copy saved {}",
                page.fetched.format("%d %b %Y %H:%M UTC")
            );
            Ok(Page {
                stale: true,
                ..page
            })
        }
        None => Err(err),
    }
}
