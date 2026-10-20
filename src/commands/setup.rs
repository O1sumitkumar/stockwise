//! `config path|show|init`: the optional config file.

use super::*;
use crate::config::{self, Source};

/// `config path`: where the config file is (or would be).
pub fn cmd_config_path() -> Result<()> {
    let path = config::path()
        .context("Cannot determine the config directory — set STOCKWISE_CONFIG to a file path")?;
    println!("{}", path.display());
    Ok(())
}

/// `config init [--force]`: write the commented template.
pub fn cmd_config_init(force: bool) -> Result<()> {
    let path = config::path()
        .context("Cannot determine the config directory — set STOCKWISE_CONFIG to a file path")?;
    config::init(&path, force)?;
    println!(
        "  {} Wrote {}",
        "✓".green(),
        path.display().to_string().cyan()
    );
    println!(
        "  {}",
        "Every line is commented out; uncomment one to change a default. \
         `stockwise config show` prints the effective values."
            .dimmed()
    );
    Ok(())
}

/// One effective setting, as `config show --json` prints it.
#[derive(Debug, Serialize)]
pub(crate) struct SettingRow {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) source: Source,
}

fn row(key: &str, value: impl Into<String>, source: Source) -> SettingRow {
    SettingRow {
        key: key.to_string(),
        value: value.into(),
        source,
    }
}

/// Effective settings. `periods` holds each `--period` command with its
/// current default (already including the config file's value).
pub(crate) fn setting_rows(
    market: Market,
    sources: config::CliSources,
    periods: &[(String, String)],
) -> Vec<SettingRow> {
    let cfg = config::get();
    let redis_source = if std::env::var_os("STOCKWISE_REDIS_URL").is_some() {
        Source::Env
    } else if cfg.redis_url.is_some() {
        Source::File
    } else {
        Source::Default
    };
    let home = crate::store::data_dir_location()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("unknown ({e:#})"));

    let mut rows = vec![
        row(
            "market",
            match market {
                Market::In => "in",
                Market::Us => "us",
            },
            sources.market,
        ),
        row("ai", crate::ai::auto_ai_enabled().to_string(), sources.ai),
        row(
            "model",
            crate::ai::model_name(),
            config::setting_source("STOCKWISE_MODEL", |c| c.model.as_ref()),
        ),
        row(
            "ollama_host",
            crate::ai::ollama_url(),
            config::setting_source("OLLAMA_HOST", |c| c.ollama_host.as_ref()),
        ),
        row("redis_url", crate::api::redis_url(), redis_source),
        row(
            "home",
            home,
            config::setting_source("STOCKWISE_HOME", |c| c.home.as_ref()),
        ),
    ];
    for (command, period) in periods {
        let source = if cfg.periods.contains_key(command) {
            Source::File
        } else {
            Source::Default
        };
        rows.push(row(&format!("periods.{}", command), period.clone(), source));
    }
    rows
}

/// `config [show]`: the config file's status and every effective setting.
pub fn cmd_config_show(
    market: Market,
    sources: config::CliSources,
    periods: &[(String, String)],
    fmt: Format,
) -> Result<()> {
    let state = config::state();
    let path = state.and_then(|s| s.path.as_ref());
    let (status, error) = match state.map(|s| &s.status) {
        Some(config::Status::Loaded) => ("loaded", None),
        Some(config::Status::Invalid(msg)) => ("invalid", Some(msg.as_str())),
        _ => ("missing", None),
    };
    let rows = setting_rows(market, sources, periods);

    if fmt == Format::Json {
        return output::print_json(&serde_json::json!({
            "path": path.map(|p| p.display().to_string()),
            "status": status,
            "error": error,
            "settings": rows,
        }));
    }

    print_header("StockWise config");
    let path_str = path.map_or("unknown (no config directory)".to_string(), |p| {
        p.display().to_string()
    });
    let note = match status {
        "loaded" => "(loaded)".green().to_string(),
        "invalid" => "(invalid — not used)".red().to_string(),
        _ => "(not found — `stockwise config init` creates one)"
            .dimmed()
            .to_string(),
    };
    print_kv("Config file", &format!("{} {}", path_str, note));
    if let Some(msg) = error {
        for line in msg.lines() {
            println!("  {:<28} {}", "", line.red());
        }
    }
    println!();
    println!(
        "  {:<28} {:<40} {}",
        "Setting".bold(),
        "Value".bold(),
        "Source".bold()
    );
    println!("  {}", "─".repeat(80).dimmed());
    for r in &rows {
        let source = match r.source {
            Source::Default => r.source.to_string().dimmed().to_string(),
            _ => r.source.to_string().cyan().to_string(),
        };
        println!("  {:<28} {:<40} {}", r.key, r.value, source);
    }
    println!(
        "\n  {}",
        "Precedence: flag > environment variable > config file > default.".dimmed()
    );
    println!();
    Ok(())
}
