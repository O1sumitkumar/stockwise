//! Local JSON persistence shared by the portfolio, watchlist, alerts,
//! simulator history, wealth history, and broker config.

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where the data directory is, without creating it: `STOCKWISE_HOME`, then
/// `home` in the config file, then `dirs::data_local_dir()/stockwise`.
pub fn data_dir_location() -> Result<PathBuf> {
    Ok(
        match crate::config::setting("STOCKWISE_HOME", |c| c.home.as_ref()) {
            Some(p) => crate::config::expand_home(&p),
            None => dirs::data_local_dir()
                .context("Cannot determine local data directory (set STOCKWISE_HOME)")?
                .join("stockwise"),
        },
    )
}

/// Directory that holds every StockWise data file (see
/// [`data_dir_location`]), created if missing.
pub fn data_dir() -> Result<PathBuf> {
    let dir = data_dir_location()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Cannot create data directory {}", dir.display()))?;
    Ok(dir)
}

/// Full path of `file` inside the data directory.
pub fn path(file: &str) -> Result<PathBuf> {
    Ok(data_dir()?.join(file))
}

/// Load `file` from the data directory, or `T::default()` if it does not exist.
pub fn load<T: DeserializeOwned + Default>(file: &str) -> Result<T> {
    load_from(&path(file)?)
}

/// Save `value` to `file` in the data directory. See [`save_to`].
pub fn save<T: Serialize>(file: &str, value: &T, private: bool) -> Result<()> {
    save_to(&path(file)?, value, private)
}

pub fn load_from<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let data =
        std::fs::read_to_string(path).with_context(|| format!("Cannot read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| {
        format!(
            "{} is not valid JSON. Fix it, or move it aside to start fresh.",
            path.display()
        )
    })
}

/// Write `value` as pretty JSON without ever leaving a half-written file:
/// write a temp file next to the target, flush it to disk, then rename it over
/// the target. With `private`, the file is readable by the owner only (0600 on
/// Unix) — used for broker credentials.
pub fn save_to<T: Serialize>(path: &Path, value: &T, private: bool) -> Result<()> {
    let data = serde_json::to_string_pretty(value)?;
    // Named portfolios live in a subdirectory (`portfolios/NAME.json`).
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Cannot create directory {}", dir.display()))?;
    }
    // Per-process temp name so a concurrent writer (e.g. the daemon) can't
    // rename our half-written file or delete it from under us.
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    // A leftover temp file would keep its old permissions on reopen.
    let _ = std::fs::remove_file(&tmp);

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;

    let mut f = opts
        .open(&tmp)
        .with_context(|| format!("Cannot write {}", tmp.display()))?;
    f.write_all(data.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("Cannot replace {}", path.display()))?;
    Ok(())
}

/// On Unix, tighten `path` to owner-only (0600) if group or others can read
/// it. Returns true if the permissions were changed.
#[cfg(unix)]
pub fn restrict_permissions(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(false);
    };
    let mode = meta.permissions().mode();
    if mode & 0o077 == 0 {
        return Ok(false);
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("Cannot restrict permissions on {}", path.display()))?;
    Ok(true)
}

#[cfg(not(unix))]
pub fn restrict_permissions(_path: &Path) -> Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Doc {
        items: Vec<String>,
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stockwise-store-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_loads_default() {
        let dir = temp_dir("missing");
        let doc: Doc = load_from(&dir.join("nope.json")).unwrap();
        assert_eq!(doc, Doc::default());
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp_file() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("doc.json");
        let doc = Doc {
            items: vec!["AAPL".into(), "M&M.NS".into()],
        };
        save_to(&path, &doc, false).unwrap();
        assert_eq!(load_from::<Doc>(&path).unwrap(), doc);
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn corrupt_file_error_names_the_path() {
        let dir = temp_dir("corrupt");
        let path = dir.join("doc.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = load_from::<Doc>(&path).unwrap_err().to_string();
        assert!(err.contains("doc.json"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn private_save_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("private");
        let path = dir.join("secret.json");
        save_to(&path, &Doc::default(), true).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(restrict_permissions(&path).unwrap());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
