//! Where broker secrets live: the OS keyring when one is available (macOS
//! Keychain, Windows Credential Manager, the Secret Service on Linux), else
//! the broker's owner-only JSON config file in the data directory.
//!
//! A config file in keyring mode keeps only non-secret settings plus
//! `"secret_store": "keyring"` and a `keyring_account` naming the keyring
//! entry; the entry holds the secret fields as one JSON object. Secrets found
//! in the file (an older file, or one the user just edited) are moved into the
//! keyring on load, and the file is rewritten without them.
//!
//! Moving happens on its own only in interactive runs ([`auto_migrate`]).
//! Set `STOCKWISE_KEYRING=off` to keep secrets in the owner-only file.
//! Tests never touch the OS keyring.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::fmt;
use std::path::Path;

/// Keyring service name for every StockWise entry.
pub const SERVICE: &str = "stockwise";
const STORE_KEY: &str = "secret_store";
const ACCOUNT_KEY: &str = "keyring_account";
const KEYRING: &str = "keyring";
/// Prefix of the placeholder values written by `trade config`.
pub const PLACEHOLDER_PREFIX: &str = "YOUR_";

/// A secret string. `Debug` never shows the value, so a stray `{:?}` can't
/// leak it into a log or an error message.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Set and not a `YOUR_...` template placeholder.
    pub fn is_real(&self) -> bool {
        is_real(&self.0)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_empty() {
            "Secret(<empty>)"
        } else {
            "Secret(***)"
        })
    }
}

/// True for a value the user filled in (not empty, not a placeholder).
pub fn is_real(value: &str) -> bool {
    !value.trim().is_empty() && !value.starts_with(PLACEHOLDER_PREFIX)
}

/// Where a broker's secrets are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Storage {
    /// In the owner-only config file.
    File,
    /// In the OS keyring entry `stockwise` / `account`.
    Keyring { account: String },
}

/// A secret store. The OS keyring in normal runs; tests use an in-memory one.
pub trait Backend: Sync {
    /// Human name, e.g. "macOS Keychain".
    fn name(&self) -> &'static str;
    /// `Err(reason)` when secrets can't be stored here.
    fn available(&self) -> std::result::Result<(), String>;
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, value: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
}

/// The platform keyring via the `keyring` crate.
struct OsKeyring;

impl OsKeyring {
    fn entry(account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, account).context("Cannot open the OS keyring")
    }
}

impl Backend for OsKeyring {
    fn name(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS Keychain"
        } else if cfg!(windows) {
            "Windows Credential Manager"
        } else {
            "Secret Service keyring"
        }
    }

    fn available(&self) -> std::result::Result<(), String> {
        keyring::Entry::store_status()
            .as_ref()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn get(&self, account: &str) -> Result<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("Cannot read from the OS keyring"),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<()> {
        Self::entry(account)?
            .set_password(value)
            .context("Cannot write to the OS keyring")
    }

    fn delete(&self, account: &str) -> Result<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("Cannot delete from the OS keyring"),
        }
    }
}

/// No keyring: secrets stay in the owner-only file.
struct Disabled(&'static str);

impl Backend for Disabled {
    fn name(&self) -> &'static str {
        "OS keyring"
    }
    fn available(&self) -> std::result::Result<(), String> {
        Err(self.0.to_string())
    }
    fn get(&self, _account: &str) -> Result<Option<String>> {
        bail!("The OS keyring is not available ({})", self.0)
    }
    fn set(&self, _account: &str, _value: &str) -> Result<()> {
        bail!("The OS keyring is not available ({})", self.0)
    }
    fn delete(&self, _account: &str) -> Result<()> {
        bail!("The OS keyring is not available ({})", self.0)
    }
}

/// `STOCKWISE_KEYRING=off` (or 0/false/no/file) keeps secrets in files.
fn disabled_by_env() -> bool {
    std::env::var("STOCKWISE_KEYRING").is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "off" | "0" | "false" | "no" | "file"
        )
    })
}

/// Whether loading a config may move its secrets into the keyring on its
/// own: only in a terminal, where the user sees the notice and can answer a
/// keychain prompt. Scripts and cron jobs keep using the file until
/// `trade config` (or any interactive trade command) migrates it.
pub fn auto_migrate() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// The secret store for this run.
pub fn backend() -> &'static dyn Backend {
    if cfg!(test) {
        return &Disabled("tests never use the OS keyring");
    }
    if disabled_by_env() {
        return &Disabled("STOCKWISE_KEYRING=off");
    }
    &OsKeyring
}

/// A broker config file loaded with its secrets merged back in.
#[derive(Debug)]
pub struct Loaded<T> {
    pub config: T,
    pub storage: Storage,
    /// Secrets were moved from the file into the keyring during this load.
    pub migrated: bool,
}

/// Load `file` from the data directory. See [`load_from`].
pub fn load<T: DeserializeOwned + Default>(
    file: &str,
    secret_keys: &[&str],
    backend: &dyn Backend,
    migrate: bool,
) -> Result<Loaded<T>> {
    load_from(&crate::store::path(file)?, secret_keys, backend, migrate)
}

/// Load a broker config, reading secrets from the keyring when the file says
/// they live there. With `migrate`, real secrets found in the file are moved
/// into the keyring (when one is available) and the file is rewritten
/// without them. A failed migration leaves the file untouched.
pub fn load_from<T: DeserializeOwned + Default>(
    path: &Path,
    secret_keys: &[&str],
    backend: &dyn Backend,
    migrate: bool,
) -> Result<Loaded<T>> {
    let name = file_label(path);
    if !path.exists() {
        return Ok(Loaded {
            config: T::default(),
            storage: Storage::File,
            migrated: false,
        });
    }
    // The file may hold a password or tokens; older versions wrote it
    // world-readable.
    if crate::store::restrict_permissions(path)? {
        eprintln!(
            "  Restricted {} to owner-only access (it was readable by other users).",
            path.display()
        );
    }
    let mut doc = match crate::store::load_from::<Value>(path)? {
        Value::Object(m) => m,
        Value::Null => Map::new(),
        _ => bail!("{} must contain a JSON object", path.display()),
    };

    let in_file = take_real_secrets(&mut doc, secret_keys);
    let account = keyring_account(&doc);
    let mut storage = match &account {
        Some(a) => Storage::Keyring { account: a.clone() },
        None => Storage::File,
    };
    let mut migrated = false;

    let mut secrets = Map::new();
    if let Storage::Keyring { account } = &storage {
        if let Err(reason) = backend.available() {
            if in_file.is_empty() {
                bail!(
                    "{name} keeps its secrets in the OS keyring, but the keyring is not available \
                     ({reason}). Unset STOCKWISE_KEYRING, or put the secrets back in {} and remove \
                     its \"{STORE_KEY}\" line.",
                    path.display()
                );
            }
        } else {
            match backend.get(account)? {
                Some(json) => {
                    secrets = parse_secret_blob(&json)
                        .with_context(|| format!("The keyring entry for {name} is corrupt"))?
                }
                None if in_file.is_empty() => bail!(
                    "The keyring has no secrets for {name} (entry {SERVICE}/{account}). \
                     Put them back in {} and run the command again.",
                    path.display()
                ),
                None => {}
            }
        }
    }

    if !in_file.is_empty() {
        // Values from the file win: the user just edited them.
        let mut merged = secrets;
        merged.extend(in_file);
        if migrate && backend.available().is_ok() {
            match move_to_keyring(path, &doc, &merged, &storage, backend) {
                Ok(account) => {
                    storage = Storage::Keyring { account };
                    migrated = true;
                    eprintln!(
                        "  Moved the secrets in {name} into the {} (the file no longer holds them).",
                        backend.name()
                    );
                }
                Err(e) => {
                    // The file is untouched and still owner-only: keep using it.
                    storage = Storage::File;
                    eprintln!(
                        "  Could not move the secrets in {name} into the {} ({e:#}); they stay in \
                         the owner-only file. Set STOCKWISE_KEYRING=off to stop trying.",
                        backend.name()
                    );
                }
            }
        } else if backend.available().is_err() {
            storage = Storage::File;
        }
        secrets = merged;
    }

    let mut full = doc;
    full.remove(STORE_KEY);
    full.remove(ACCOUNT_KEY);
    full.extend(secrets);
    let config = serde_json::from_value(Value::Object(full))
        .with_context(|| format!("{} has an invalid value", path.display()))?;
    Ok(Loaded {
        config,
        storage,
        migrated,
    })
}

/// Save `value` to `file` in the data directory. See [`save_to`].
pub fn save<T: Serialize>(
    file: &str,
    value: &T,
    secret_keys: &[&str],
    storage: &Storage,
    backend: &dyn Backend,
) -> Result<()> {
    save_to(
        &crate::store::path(file)?,
        value,
        secret_keys,
        storage,
        backend,
    )
}

/// Save a broker config. In keyring mode the secret fields go to the keyring
/// entry and the file gets the rest; in file mode everything goes to the
/// owner-only file.
pub fn save_to<T: Serialize>(
    path: &Path,
    value: &T,
    secret_keys: &[&str],
    storage: &Storage,
    backend: &dyn Backend,
) -> Result<()> {
    let Value::Object(mut doc) = serde_json::to_value(value)? else {
        bail!("Broker config must serialize to a JSON object");
    };
    match storage {
        Storage::File => crate::store::save_to(path, &Value::Object(doc), true),
        Storage::Keyring { account } => {
            let secrets: Map<String, Value> = secret_keys
                .iter()
                .filter_map(|k| doc.remove(*k).map(|v| (k.to_string(), v)))
                .collect();
            write_verified(backend, account, &secrets)?;
            doc.insert(STORE_KEY.into(), Value::String(KEYRING.into()));
            doc.insert(ACCOUNT_KEY.into(), Value::String(account.clone()));
            crate::store::save_to(path, &Value::Object(doc), true)
        }
    }
}

/// Move every secret of a file-mode config into the keyring now. Returns the
/// storage afterwards. Errors (with the reason) when no keyring is available.
pub fn migrate_file(path: &Path, secret_keys: &[&str], backend: &dyn Backend) -> Result<Storage> {
    if let Err(reason) = backend.available() {
        bail!("No OS keyring is available here ({reason}); secrets stay in the owner-only file.");
    }
    if !path.exists() {
        bail!("{} does not exist yet", path.display());
    }
    // load_from with migrate=true does the move.
    let loaded: Loaded<Value> = load_from(path, secret_keys, backend, true)?;
    Ok(loaded.storage)
}

/// Delete the keyring entry for a config, if it uses one.
pub fn forget(storage: &Storage, backend: &dyn Backend) -> Result<()> {
    match storage {
        Storage::File => Ok(()),
        Storage::Keyring { account } => backend.delete(account),
    }
}

fn move_to_keyring(
    path: &Path,
    doc: &Map<String, Value>,
    secrets: &Map<String, Value>,
    storage: &Storage,
    backend: &dyn Backend,
) -> Result<String> {
    let account = match storage {
        Storage::Keyring { account } => account.clone(),
        Storage::File => new_account(path),
    };
    write_verified(backend, &account, secrets)?;
    let mut file_doc = doc.clone();
    file_doc.insert(STORE_KEY.into(), Value::String(KEYRING.into()));
    file_doc.insert(ACCOUNT_KEY.into(), Value::String(account.clone()));
    crate::store::save_to(path, &Value::Object(file_doc), true)?;
    Ok(account)
}

/// Write the secrets and read them back, so a keyring that silently drops
/// writes can't make us delete the only copy from the file.
fn write_verified(
    backend: &dyn Backend,
    account: &str,
    secrets: &Map<String, Value>,
) -> Result<()> {
    let json = serde_json::to_string(secrets)?;
    backend.set(account, &json)?;
    match backend.get(account)? {
        Some(back) if back == json => Ok(()),
        _ => bail!("The OS keyring did not return what was written to it"),
    }
}

/// Remove and return the secret fields that hold real (non-placeholder,
/// non-empty) values. Placeholders and empty values stay in the file.
fn take_real_secrets(doc: &mut Map<String, Value>, secret_keys: &[&str]) -> Map<String, Value> {
    let mut out = Map::new();
    for key in secret_keys {
        let real = matches!(doc.get(*key), Some(Value::String(s)) if is_real(s));
        if real && let Some(v) = doc.remove(*key) {
            out.insert(key.to_string(), v);
        }
    }
    out
}

fn keyring_account(doc: &Map<String, Value>) -> Option<String> {
    if doc.get(STORE_KEY).and_then(Value::as_str) != Some(KEYRING) {
        return None;
    }
    doc.get(ACCOUNT_KEY)
        .and_then(Value::as_str)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
}

fn parse_secret_blob(json: &str) -> Result<Map<String, Value>> {
    match serde_json::from_str(json)? {
        Value::Object(m) => Ok(m),
        _ => bail!("expected a JSON object"),
    }
}

/// A keyring account unique to this config file: the broker name plus a
/// random-enough suffix, so two data directories (`STOCKWISE_HOME`) never
/// share an entry.
fn new_account(path: &Path) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write(path.as_os_str().as_encoded_bytes());
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    h.write_u32(std::process::id());
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("broker")
        .trim_end_matches("_config");
    format!("{stem}-{:016x}", h.finish())
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// One-line description of where secrets live, for `doctor` and `trade config`.
pub fn describe(storage: &Storage, backend: &dyn Backend) -> String {
    match storage {
        Storage::Keyring { .. } => format!("secrets in the {}", backend.name()),
        Storage::File => match backend.available() {
            Ok(()) => format!(
                "secrets in the owner-only file (run `trade config --keyring` to move them into the {})",
                backend.name()
            ),
            Err(reason) => format!("secrets in the owner-only file (no keyring: {reason})"),
        },
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory keyring for tests.
    #[derive(Default)]
    pub struct MemoryKeyring {
        pub entries: Mutex<HashMap<String, String>>,
        pub fail_writes: bool,
    }

    impl Backend for MemoryKeyring {
        fn name(&self) -> &'static str {
            "test keyring"
        }
        fn available(&self) -> std::result::Result<(), String> {
            Ok(())
        }
        fn get(&self, account: &str) -> Result<Option<String>> {
            Ok(self.entries.lock().unwrap().get(account).cloned())
        }
        fn set(&self, account: &str, value: &str) -> Result<()> {
            if self.fail_writes {
                bail!("write refused");
            }
            self.entries
                .lock()
                .unwrap()
                .insert(account.into(), value.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<()> {
            self.entries.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Cfg {
        #[serde(default)]
        client_id: String,
        #[serde(default)]
        password: Secret,
        #[serde(default)]
        totp_secret: Secret,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_order_value: Option<f64>,
    }

    const KEYS: &[&str] = &["password", "totp_secret"];

    fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stockwise-secrets-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("angel_config.json");
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn file_json(path: &Path) -> Map<String, Value> {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    const PLAIN: &str = r#"{"client_id":"A123","password":"1234","totp_secret":"JBSWY3DPEHPK3PXP","max_order_value":50000}"#;

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert!(!format!("{:?}", Cfg::default()).contains("hunter2"));
    }

    #[test]
    fn migrates_plaintext_secrets_into_keyring() {
        let path = temp_file("migrate", PLAIN);
        let kr = MemoryKeyring::default();
        let loaded: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        assert!(loaded.migrated);
        assert_eq!(loaded.config.password.expose(), "1234");
        assert_eq!(loaded.config.max_order_value, Some(50000.0));
        let Storage::Keyring { account } = &loaded.storage else {
            panic!("expected keyring storage");
        };
        assert!(account.starts_with("angel-"));

        // The file keeps the settings but no secrets.
        let doc = file_json(&path);
        assert!(!doc.contains_key("password") && !doc.contains_key("totp_secret"));
        assert_eq!(doc["client_id"], "A123");
        assert_eq!(doc[STORE_KEY], KEYRING);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("1234") && !raw.contains("JBSWY3DPEHPK3PXP"));

        // A second load reads the secrets back from the keyring.
        let again: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        assert!(!again.migrated);
        assert_eq!(again.config, loaded.config);
    }

    #[test]
    fn no_keyring_keeps_the_file_as_is() {
        let path = temp_file("nokeyring", PLAIN);
        let loaded: Loaded<Cfg> = load_from(&path, KEYS, &Disabled("headless"), true).unwrap();
        assert!(!loaded.migrated);
        assert_eq!(loaded.storage, Storage::File);
        assert_eq!(loaded.config.totp_secret.expose(), "JBSWY3DPEHPK3PXP");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), PLAIN);
    }

    #[test]
    fn failed_keyring_write_keeps_the_file() {
        let path = temp_file("failwrite", PLAIN);
        let kr = MemoryKeyring {
            fail_writes: true,
            ..Default::default()
        };
        let loaded: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        assert!(!loaded.migrated);
        assert_eq!(loaded.storage, Storage::File);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), PLAIN);
    }

    #[test]
    fn placeholders_are_not_migrated() {
        let path = temp_file(
            "placeholder",
            r#"{"client_id":"YOUR_CLIENT_ID","password":"YOUR_PASSWORD","totp_secret":""}"#,
        );
        let kr = MemoryKeyring::default();
        let loaded: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        assert!(!loaded.migrated);
        assert!(kr.entries.lock().unwrap().is_empty());
        assert_eq!(loaded.config.password.expose(), "YOUR_PASSWORD");
    }

    #[test]
    fn edited_secret_in_keyring_mode_updates_the_keyring() {
        let path = temp_file("edit", PLAIN);
        let kr = MemoryKeyring::default();
        let first: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        // The user pastes a new password into the (keyring-mode) file.
        let mut doc = file_json(&path);
        doc.insert("password".into(), Value::String("9999".into()));
        std::fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();

        let second: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        assert_eq!(second.storage, first.storage);
        assert_eq!(second.config.password.expose(), "9999");
        assert_eq!(second.config.totp_secret.expose(), "JBSWY3DPEHPK3PXP");
        assert!(!file_json(&path).contains_key("password"));
    }

    #[test]
    fn keyring_mode_without_keyring_is_a_clear_error() {
        let path = temp_file("missingkr", PLAIN);
        let kr = MemoryKeyring::default();
        let _: Loaded<Cfg> = load_from(&path, KEYS, &kr, true).unwrap();
        let err = load_from::<Cfg>(&path, KEYS, &Disabled("STOCKWISE_KEYRING=off"), true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("keyring is not available"), "{err}");
    }

    #[test]
    fn save_in_keyring_mode_splits_secrets_out() {
        let path = temp_file("save", "{}");
        let kr = MemoryKeyring::default();
        let storage = Storage::Keyring {
            account: "angel-test".into(),
        };
        let cfg = Cfg {
            client_id: "A1".into(),
            password: Secret::new("pw"),
            totp_secret: Secret::new("TOTP"),
            max_order_value: None,
        };
        save_to(&path, &cfg, KEYS, &storage, &kr).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("pw\"") && !raw.contains("TOTP"));
        let back: Loaded<Cfg> = load_from(&path, KEYS, &kr, false).unwrap();
        assert_eq!(back.config, cfg);
    }

    #[test]
    fn missing_file_is_default_file_storage() {
        let dir =
            std::env::temp_dir().join(format!("stockwise-secrets-none-{}", std::process::id()));
        let loaded: Loaded<Cfg> = load_from(
            &dir.join("nope.json"),
            KEYS,
            &MemoryKeyring::default(),
            true,
        )
        .unwrap();
        assert_eq!(loaded.storage, Storage::File);
        assert_eq!(loaded.config, Cfg::default());
    }
}
