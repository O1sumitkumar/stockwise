//! `stockwise serve`: a read-only JSON API over the same views the CLI
//! prints, plus Prometheus metrics. There are no trading or broker
//! endpoints, and broker credentials are never read.
//!
//! Listens on loopback by default. A non-loopback bind needs
//! `--allow-remote` and a bearer token in `STOCKWISE_API_TOKEN`.

mod metrics;

use crate::market::{self, Market};
use crate::views::{self, DataSource};
use anyhow::{Context, Result, bail};
use axum::Router;
use axum::extract::{MatchedPath, Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use clap::ValueEnum;
use colored::Colorize;
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

pub use metrics::Metrics;

/// Most symbols one `/api/quote` request may ask for.
const MAX_SYMBOLS: usize = 50;
/// Shortest accepted `STOCKWISE_API_TOKEN`.
const MIN_TOKEN_LEN: usize = 16;

pub struct AppState<D> {
    pub source: D,
    /// Market for bare tickers when a request has no `market=` parameter.
    pub market: Market,
    /// Bearer token required on every route except `/healthz`.
    pub token: Option<String>,
    pub metrics: Metrics,
}

type Shared<D> = State<Arc<AppState<D>>>;

/// JSON error body: `{"error": "..."}`.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    /// Market data or a local data file could not be read.
    Data(anyhow::Error),
}

impl ApiError {
    fn from_data(e: anyhow::Error) -> Self {
        match e.downcast_ref::<views::NotFound>() {
            Some(nf) => Self::NotFound(nf.to_string()),
            None => Self::Data(e),
        }
    }
}

/// Drop the Yahoo session crumb from error text before it leaves the process.
fn redact(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while let Some(i) = rest.find("crumb=") {
        out.push_str(&rest[..i + "crumb=".len()]);
        out.push_str("REDACTED");
        rest = &rest[i + "crumb=".len()..];
        let end = rest.find(['&', ')', ' ']).unwrap_or(rest.len());
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::Data(e) => {
                let msg = redact(&format!("{:#}", e));
                eprintln!("  {} {}", "error:".red(), msg);
                (StatusCode::BAD_GATEWAY, msg)
            }
        };
        (status, axum::Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

type ApiResult = std::result::Result<Response, ApiError>;

fn json<T: serde::Serialize>(v: T) -> ApiResult {
    Ok(axum::Json(v).into_response())
}

#[derive(Deserialize)]
struct MarketParam {
    market: Option<String>,
}

impl<D> AppState<D> {
    fn market_for(&self, param: Option<&str>) -> std::result::Result<Market, ApiError> {
        match param {
            None | Some("") => Ok(self.market),
            Some(m) => Market::from_str(m, true).map_err(|_| {
                ApiError::BadRequest(format!("Unknown market '{}' (use in or us)", m))
            }),
        }
    }
}

/// A ticker as typed by a user: short, and only characters Yahoo symbols use.
fn check_symbol(s: &str) -> std::result::Result<(), ApiError> {
    let ok = !s.is_empty()
        && s.len() <= 32
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || ".^=-&_".contains(c));
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!("Invalid symbol '{}'", s)))
    }
}

#[derive(Deserialize)]
struct QuoteParams {
    symbols: Option<String>,
    market: Option<String>,
}

/// `GET /api/quote?symbols=RELIANCE,TCS` — the `quote --json` array.
/// Symbols Yahoo doesn't know are listed in the `X-Stockwise-Missing` header;
/// 404 if none are known.
async fn quote<D: DataSource>(State(st): Shared<D>, Query(p): Query<QuoteParams>) -> ApiResult {
    let market = st.market_for(p.market.as_deref())?;
    let raw: Vec<&str> = p
        .symbols
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if raw.is_empty() {
        return Err(ApiError::BadRequest(
            "symbols is required, e.g. /api/quote?symbols=RELIANCE,TCS".into(),
        ));
    }
    if raw.len() > MAX_SYMBOLS {
        return Err(ApiError::BadRequest(format!(
            "At most {} symbols per request",
            MAX_SYMBOLS
        )));
    }
    for s in &raw {
        check_symbol(s)?;
    }
    let resolved: Vec<String> = raw
        .iter()
        .map(|s| market::resolve_symbol(s, market))
        .collect();
    let set = views::quotes(&st.source, &resolved)
        .await
        .map_err(ApiError::from_data)?;
    if set.quotes.is_empty() {
        return Err(ApiError::NotFound(format!(
            "No quote for {}",
            set.missing.join(", ")
        )));
    }
    let mut resp = axum::Json(&set.quotes).into_response();
    if !set.missing.is_empty()
        && let Ok(v) = HeaderValue::from_str(&set.missing.join(","))
    {
        resp.headers_mut().insert("x-stockwise-missing", v);
    }
    Ok(resp)
}

#[derive(Deserialize)]
struct HistoryParams {
    period: Option<String>,
    market: Option<String>,
}

/// `GET /api/history/{symbol}?period=1mo` — the `history --json` object.
async fn history<D: DataSource>(
    State(st): Shared<D>,
    Path(symbol): Path<String>,
    Query(p): Query<HistoryParams>,
) -> ApiResult {
    let market = st.market_for(p.market.as_deref())?;
    check_symbol(&symbol)?;
    let period = p.period.as_deref().unwrap_or("1mo");
    views::check_period(period).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let resolved = market::resolve_symbol(&symbol, market);
    let hist = views::history(&st.source, &resolved, period, market)
        .await
        .map_err(ApiError::from_data)?;
    json(hist)
}

/// `GET /api/markets` — the `markets --json` array.
async fn markets<D: DataSource>(State(st): Shared<D>, Query(p): Query<MarketParam>) -> ApiResult {
    let market = st.market_for(p.market.as_deref())?;
    json(
        views::markets(&st.source, market)
            .await
            .map_err(ApiError::from_data)?,
    )
}

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
}

/// `GET /api/search?q=tata motors` — the `search --json` array.
async fn search<D: DataSource>(State(st): Shared<D>, Query(p): Query<SearchParams>) -> ApiResult {
    let q = p.q.as_deref().unwrap_or("").trim();
    if q.is_empty() || q.len() > 100 {
        return Err(ApiError::BadRequest(
            "q is required (1-100 characters), e.g. /api/search?q=tata".into(),
        ));
    }
    json(st.source.search(q).await.map_err(ApiError::from_data)?)
}

/// `GET /api/portfolio` — holdings valued at live prices, with totals.
async fn portfolio<D: DataSource>(State(st): Shared<D>) -> ApiResult {
    json(
        views::portfolio(&st.source)
            .await
            .map_err(ApiError::from_data)?,
    )
}

/// `GET /api/watchlist` — watched symbols with live prices.
async fn watchlist<D: DataSource>(State(st): Shared<D>) -> ApiResult {
    json(
        views::watchlist(&st.source)
            .await
            .map_err(ApiError::from_data)?,
    )
}

/// `GET /api/alerts` — every alert, with the live price and whether it fired.
async fn alerts<D: DataSource>(State(st): Shared<D>) -> ApiResult {
    json(
        views::alerts(&st.source)
            .await
            .map_err(ApiError::from_data)?,
    )
}

async fn healthz() -> Response {
    axum::Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
    .into_response()
}

async fn prometheus<D: DataSource>(State(st): Shared<D>) -> Response {
    let daemon = st.source.daemon_status();
    let body = st
        .metrics
        .render(daemon.as_ref(), chrono::Utc::now().timestamp());
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

async fn not_found() -> Response {
    ApiError::NotFound("No such endpoint — see docs/server.md".into()).into_response()
}

/// Compare without leaking the matching prefix length through timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn require_token<D: DataSource>(State(st): Shared<D>, req: Request, next: Next) -> Response {
    if let Some(token) = &st.token {
        let presented = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim);
        if !presented.is_some_and(|t| constant_time_eq(t.as_bytes(), token.as_bytes())) {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                axum::Json(serde_json::json!({ "error": "Missing or invalid bearer token" })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

/// Count and time every request; mark responses private and uncacheable.
async fn track<D: DataSource>(State(st): Shared<D>, req: Request, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "unmatched".into());
    let method = req.method().clone();
    let started = Instant::now();
    let mut resp = next.run(req).await;
    st.metrics.observe(
        method.as_str(),
        &route,
        resp.status().as_u16(),
        started.elapsed(),
    );
    let headers = resp.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    resp
}

pub fn router<D: DataSource + 'static>(state: Arc<AppState<D>>) -> Router {
    let protected = Router::new()
        .route("/api/quote", get(quote::<D>))
        .route("/api/history/{symbol}", get(history::<D>))
        .route("/api/markets", get(markets::<D>))
        .route("/api/search", get(search::<D>))
        .route("/api/portfolio", get(portfolio::<D>))
        .route("/api/watchlist", get(watchlist::<D>))
        .route("/api/alerts", get(alerts::<D>))
        .route("/metrics", get(prometheus::<D>))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_token::<D>,
        ));
    Router::new()
        .route("/healthz", get(healthz))
        .merge(protected)
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(state.clone(), track::<D>))
        .with_state(state)
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// Refuse to expose the API beyond this machine unless asked to, and then
/// only behind a token.
pub fn check_bind(addrs: &[SocketAddr], allow_remote: bool, token: Option<&str>) -> Result<()> {
    if addrs.is_empty() {
        bail!("--bind did not resolve to any address");
    }
    if let Some(t) = token
        && t.len() < MIN_TOKEN_LEN
    {
        bail!(
            "STOCKWISE_API_TOKEN is too short — use at least {} random characters (e.g. `openssl rand -hex 32`)",
            MIN_TOKEN_LEN
        );
    }
    let Some(remote) = addrs.iter().find(|a| !is_loopback(a.ip())) else {
        return Ok(());
    };
    if !allow_remote {
        bail!(
            "Refusing to listen on {}: other machines could read your portfolio.\n  \
             Keep the default 127.0.0.1:8787, or pass --allow-remote and set STOCKWISE_API_TOKEN \
             to require a bearer token.",
            remote
        );
    }
    if token.is_none() {
        bail!(
            "--allow-remote needs STOCKWISE_API_TOKEN: every request must then send \
             `Authorization: Bearer <token>`"
        );
    }
    Ok(())
}

pub async fn run(bind: &str, allow_remote: bool, market: Market) -> Result<()> {
    let token = std::env::var("STOCKWISE_API_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(bind)
        .await
        .with_context(|| {
            format!(
                "Cannot resolve --bind {} (use HOST:PORT, e.g. 127.0.0.1:8787)",
                bind
            )
        })?
        .collect();
    check_bind(&addrs, allow_remote, token.as_deref())?;
    let listener = tokio::net::TcpListener::bind(&addrs[..])
        .await
        .with_context(|| format!("Cannot listen on {}", bind))?;
    let local = listener.local_addr()?;

    crate::display::print_header("StockWise API — read-only");
    crate::display::print_kv(
        "Listening on",
        &format!("http://{}", local).cyan().to_string(),
    );
    crate::display::print_kv(
        "Market",
        match market {
            Market::In => "in (bare tickers get .NS; override with ?market=us)",
            Market::Us => "us (override with ?market=in)",
        },
    );
    crate::display::print_kv(
        "Auth",
        if token.is_some() {
            "bearer token required (STOCKWISE_API_TOKEN)"
        } else {
            "none — loopback only"
        },
    );
    println!(
        "\n  {}\n  {}\n  {}\n",
        "/api/quote?symbols=  /api/history/{symbol}?period=  /api/markets  /api/search?q=".dimmed(),
        "/api/portfolio  /api/watchlist  /api/alerts  /healthz  /metrics".dimmed(),
        "No trading endpoints. Press Ctrl+C to stop.".dimmed()
    );

    let state = Arc::new(AppState {
        source: views::LiveSource::new(),
        market,
        token,
        metrics: Metrics::new(),
    });
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("Server error")?;
    println!("\n  Stopped.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon_status::DaemonStatus;
    use crate::views::fixtures::Fixture;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn app_with(fixture: Fixture, token: Option<&str>) -> Router {
        router(Arc::new(AppState {
            source: fixture,
            market: Market::In,
            token: token.map(str::to_string),
            metrics: Metrics::new(),
        }))
    }

    fn app() -> Router {
        app_with(Fixture::default(), None)
    }

    async fn get_with(app: &Router, uri: &str, auth: Option<&str>) -> (StatusCode, Response) {
        let mut req = Request::builder().uri(uri);
        if let Some(a) = auth {
            req = req.header(header::AUTHORIZATION, a);
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        (resp.status(), resp)
    }

    async fn body(resp: Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let (status, resp) = get_with(app, uri, None).await;
        (status, serde_json::from_str(&body(resp).await).unwrap())
    }

    #[tokio::test]
    async fn healthz_is_ok() {
        let (status, v) = get_json(&app(), "/healthz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["status"], "ok");
    }

    #[tokio::test]
    async fn quote_resolves_bare_tickers_and_reports_missing() {
        let app = app();
        let (status, resp) = get_with(&app, "/api/quote?symbols=tcs,%20NOPE", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(resp.headers()["x-stockwise-missing"], "NOPE.NS");
        assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store");
        let v: serde_json::Value = serde_json::from_str(&body(resp).await).unwrap();
        // Same objects as `quote --json`: Yahoo's camelCase fields.
        assert_eq!(v[0]["symbol"], "TCS.NS");
        assert_eq!(v[0]["regularMarketPrice"], 4100.0);
        assert_eq!(v.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn quote_errors() {
        let app = app();
        assert_eq!(
            get_json(&app, "/api/quote").await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            get_json(&app, "/api/quote?symbols=a%20b").await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            get_json(&app, "/api/quote?symbols=TCS&market=xx").await.0,
            StatusCode::BAD_REQUEST
        );
        let many = vec!["A"; MAX_SYMBOLS + 1].join(",");
        assert_eq!(
            get_json(&app, &format!("/api/quote?symbols={many}"))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let (status, v) = get_json(&app, "/api/quote?symbols=NOPE").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(v["error"], "No quote for NOPE.NS");
    }

    #[tokio::test]
    async fn quote_market_param_overrides_default() {
        let (status, v) = get_json(&app(), "/api/quote?symbols=^GSPC&market=us").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v[0]["symbol"], "^GSPC");
    }

    #[tokio::test]
    async fn history_matches_cli_json() {
        let (status, v) = get_json(&app(), "/api/history/TCS?period=1mo").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["symbol"], "TCS.NS");
        assert_eq!(v["interval"], "1d");
        assert_eq!(v["period"], "1mo");
        assert_eq!(v["bars"].as_array().unwrap().len(), 5);
        assert_eq!(v["bars"][4]["close"], 107.5);
        assert!(v["bars"][0].get("ts").is_none());
    }

    #[tokio::test]
    async fn history_errors() {
        let app = app();
        assert_eq!(
            get_json(&app, "/api/history/TCS?period=7x").await.0,
            StatusCode::BAD_REQUEST
        );
        // Unknown symbol: Yahoo answers 404, which surfaces as a data error.
        let (status, v) = get_json(&app, "/api/history/NOPE").await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert!(v["error"].as_str().unwrap().contains("404"));
    }

    #[tokio::test]
    async fn markets_search_portfolio_watchlist_alerts() {
        let app = app();
        let (s, v) = get_json(&app, "/api/markets").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v[0]["name"], "NIFTY 50");
        assert_eq!(v[0]["region"], "Indian Markets");

        let (s, v) = get_json(&app, "/api/search?q=reliance").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v[0]["symbol"], "RELIANCE.NS");
        assert_eq!(
            get_json(&app, "/api/search?q=").await.0,
            StatusCode::BAD_REQUEST
        );

        let (s, v) = get_json(&app, "/api/portfolio").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["total_pnl"], 5500.0);
        assert_eq!(v["unpriced"][0], "DELISTED.NS");

        let (s, v) = get_json(&app, "/api/watchlist").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v[0]["symbol"], "RELIANCE.NS");
        assert_eq!(v[0]["price"], 1300.0);

        let (s, v) = get_json(&app, "/api/alerts").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v[0]["triggered"], true);
        assert_eq!(v[2]["triggered"], false);
        assert!(v[2]["price"].is_null());
    }

    #[tokio::test]
    async fn upstream_failure_is_502_without_the_crumb() {
        let f = Fixture::default();
        *f.fail.lock().unwrap() = Some(
            "error sending request for url (https://x.yahoo.com/q?symbols=A&crumb=s3cr3t)".into(),
        );
        let (status, v) = get_json(&app_with(f, None), "/api/markets").await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        let msg = v["error"].as_str().unwrap();
        assert!(!msg.contains("s3cr3t"), "{msg}");
        assert!(msg.contains("crumb=REDACTED"), "{msg}");
    }

    #[tokio::test]
    async fn unknown_routes_and_methods() {
        let app = app();
        assert_eq!(get_json(&app, "/api/trade").await.0, StatusCode::NOT_FOUND);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/portfolio")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn token_guards_everything_but_healthz() {
        let token = "0123456789abcdef0123";
        let app = app_with(Fixture::default(), Some(token));
        for uri in ["/api/portfolio", "/metrics", "/api/quote?symbols=TCS"] {
            let (status, resp) = get_with(&app, uri, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
            assert_eq!(resp.headers()[header::WWW_AUTHENTICATE], "Bearer");
            let (status, _) = get_with(&app, uri, Some("Bearer wrong")).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
            let (status, _) = get_with(&app, uri, Some(&format!("Bearer {token}"))).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
        }
        assert_eq!(get_with(&app, "/healthz", None).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn metrics_count_routes_and_report_the_daemon() {
        let f = Fixture {
            daemon: Some(DaemonStatus {
                updated_at: chrono::Utc::now().timestamp(),
                open_positions: 3,
                day_pnl: 420.5,
                kill_switch: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        let app = app_with(f, None);
        get_json(&app, "/api/history/TCS").await;
        get_json(&app, "/api/history/INFY").await;
        get_json(&app, "/nope").await;
        let (status, resp) = get_with(&app, "/metrics", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            resp.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/plain; version=0.0.4")
        );
        let text = body(resp).await;
        assert!(
            text.contains(
                "stockwise_http_requests_total{method=\"GET\",route=\"/api/history/{symbol}\",status=\"200\"} 2"
            ),
            "{text}"
        );
        assert!(text.contains(
            "stockwise_http_requests_total{method=\"GET\",route=\"unmatched\",status=\"404\"} 1"
        ));
        assert!(text.contains("stockwise_daemon_up 1"));
        assert!(text.contains("stockwise_daemon_open_positions 3"));
        assert!(text.contains("stockwise_daemon_day_pnl 420.5"));
        assert!(text.contains("stockwise_daemon_kill_switch 0"));
        assert!(text.contains("stockwise_upstream_errors_total "));
        assert!(text.contains("stockwise_cache_hits_total{layer=\"memory\"} "));
    }

    #[test]
    fn bind_rules() {
        let local: SocketAddr = "127.0.0.1:8787".parse().unwrap();
        let local6: SocketAddr = "[::1]:8787".parse().unwrap();
        let mapped: SocketAddr = "[::ffff:127.0.0.1]:8787".parse().unwrap();
        let any: SocketAddr = "0.0.0.0:8787".parse().unwrap();
        let lan: SocketAddr = "192.168.1.5:8787".parse().unwrap();
        let token = "0123456789abcdef";

        assert!(check_bind(&[local], false, None).is_ok());
        assert!(check_bind(&[local6, mapped], false, None).is_ok());
        assert!(check_bind(&[], false, None).is_err());
        for addr in [any, lan] {
            let err = check_bind(&[addr], false, Some(token))
                .unwrap_err()
                .to_string();
            assert!(err.contains("--allow-remote"), "{err}");
            let err = check_bind(&[addr], true, None).unwrap_err().to_string();
            assert!(err.contains("STOCKWISE_API_TOKEN"), "{err}");
            assert!(check_bind(&[addr], true, Some(token)).is_ok());
        }
        // A localhost name that also resolves to a LAN address is remote.
        assert!(check_bind(&[local, lan], false, None).is_err());
        assert!(check_bind(&[local], false, Some("short")).is_err());
    }

    #[test]
    fn redact_strips_every_crumb() {
        assert_eq!(
            redact("url (https://a?x=1&crumb=abc) and crumb=def&y=2"),
            "url (https://a?x=1&crumb=REDACTED) and crumb=REDACTED&y=2"
        );
        assert_eq!(redact("no secrets"), "no secrets");
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
