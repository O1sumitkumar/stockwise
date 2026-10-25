//! Long-running interfaces: the full-screen `tui` and the HTTP `serve` mode.
//! Both are read-only and sit behind cargo features (on by default).

use super::*;

#[cfg(feature = "tui")]
pub async fn cmd_tui(interval_secs: u64, snapshot: Option<&str>, market: Market) -> Result<()> {
    let snapshot = snapshot.map(crate::tui::parse_size).transpose()?;
    crate::tui::run(
        market,
        crate::tui::Options {
            interval: std::time::Duration::from_secs(interval_secs),
            snapshot,
        },
    )
    .await
}

#[cfg(not(feature = "tui"))]
pub async fn cmd_tui(_interval_secs: u64, _snapshot: Option<&str>, _market: Market) -> Result<()> {
    bail!(
        "This stockwise was built without the `tui` feature. Reinstall with \
         `cargo install stockwise` (default features), or use `stockwise dashboard`."
    )
}

#[cfg(feature = "server")]
pub async fn cmd_serve(bind: &str, allow_remote: bool, open: bool, market: Market) -> Result<()> {
    crate::server::run(bind, allow_remote, market, open).await
}

#[cfg(not(feature = "server"))]
pub async fn cmd_serve(
    _bind: &str,
    _allow_remote: bool,
    _open: bool,
    _market: Market,
) -> Result<()> {
    bail!(
        "This stockwise was built without the `server` feature. Reinstall with \
         `cargo install stockwise` (default features)."
    )
}
