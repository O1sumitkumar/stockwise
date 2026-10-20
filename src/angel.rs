use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const BASE_URL: &str = "https://apiconnect.angelone.in";
pub const CONFIG_FILE: &str = "angel_config.json";
/// Prefix of the placeholder values written by `trade config`.
const PLACEHOLDER_PREFIX: &str = "YOUR_";

// ── Config persistence ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AngelConfig {
    pub api_key: String,
    pub client_id: String,
    pub password: String,
    pub totp_secret: String,
    /// Refuse any single order whose estimated value exceeds this (₹).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_order_value: Option<f64>,
}

impl AngelConfig {
    pub fn load() -> Result<Self> {
        let path = crate::store::path(CONFIG_FILE)?;
        // The file holds a password and TOTP secret; older versions wrote it
        // world-readable.
        if crate::store::restrict_permissions(&path)? {
            eprintln!(
                "  Restricted {} to owner-only access (it was readable by other users).",
                path.display()
            );
        }
        crate::store::load_from(&path)
    }

    pub fn save(&self) -> Result<()> {
        crate::store::save(CONFIG_FILE, self, true)
    }

    pub fn is_configured(&self) -> bool {
        let real = |v: &str| !v.is_empty() && !v.starts_with(PLACEHOLDER_PREFIX);
        real(&self.api_key)
            && real(&self.client_id)
            && real(&self.password)
            && real(&self.totp_secret)
    }

    /// Template with placeholder values for the user to edit.
    pub fn template() -> Self {
        Self {
            api_key: format!("{PLACEHOLDER_PREFIX}API_KEY"),
            client_id: format!("{PLACEHOLDER_PREFIX}CLIENT_ID"),
            password: format!("{PLACEHOLDER_PREFIX}PASSWORD_OR_PIN"),
            totp_secret: format!("{PLACEHOLDER_PREFIX}TOTP_SECRET"),
            max_order_value: None,
        }
    }
}

// ── TOTP generation ──

fn generate_totp(secret: &str) -> Result<String> {
    // TOTP: RFC 6238, base32-encoded secret, 30-second window, 6 digits
    let secret_bytes = base32_decode(secret)?;
    let time_step = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        / 30;

    let msg = time_step.to_be_bytes();
    let hash = hmac_sha1(&secret_bytes, &msg);

    let offset = (hash[19] & 0x0f) as usize;
    let code = ((hash[offset] as u32 & 0x7f) << 24
        | (hash[offset + 1] as u32) << 16
        | (hash[offset + 2] as u32) << 8
        | hash[offset + 3] as u32)
        % 1_000_000;

    Ok(format!("{:06}", code))
}

fn base32_decode(input: &str) -> Result<Vec<u8>> {
    let input = input.trim().to_uppercase().replace(' ', "");
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = 0u64;
    let mut bit_count = 0;
    let mut output = Vec::new();

    for ch in input.bytes() {
        if ch == b'=' {
            break;
        }
        let val = alphabet
            .iter()
            .position(|&c| c == ch)
            .context("Invalid base32 character")? as u64;
        bits = (bits << 5) | val;
        bit_count += 5;
        if bit_count >= 8 {
            bit_count -= 8;
            output.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }
    Ok(output)
}

fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let block_size = 64;
    let mut k = vec![0u8; block_size];
    if key.len() > block_size {
        k[..20].copy_from_slice(&sha1(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut i_pad = vec![0x36u8; block_size];
    let mut o_pad = vec![0x5cu8; block_size];
    for j in 0..block_size {
        i_pad[j] ^= k[j];
        o_pad[j] ^= k[j];
    }

    i_pad.extend_from_slice(msg);
    let inner_hash = sha1(&i_pad);
    o_pad.extend_from_slice(&inner_hash);
    sha1(&o_pad)
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

// ── API Client ──

pub struct AngelClient {
    client: reqwest::Client,
    config: AngelConfig,
    jwt_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    status: bool,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct LoginData {
    #[serde(rename = "jwtToken")]
    jwt_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Holding {
    pub tradingsymbol: Option<String>,
    pub exchange: Option<String>,
    pub quantity: Option<i64>,
    pub averageprice: Option<f64>,
    pub ltp: Option<f64>,
    pub pnl: Option<f64>,
    pub pnlpercentage: Option<f64>,
    pub symboltoken: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub tradingsymbol: Option<String>,
    pub exchange: Option<String>,
    pub quantity: Option<String>,
    pub buyavgprice: Option<String>,
    pub ltp: Option<String>,
    pub pnl: Option<String>,
    pub symboltoken: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HoldingsResponse {
    holdings: Option<Vec<Holding>>,
}

#[derive(Debug, Serialize)]
struct OrderRequest {
    variety: String,
    tradingsymbol: String,
    symboltoken: String,
    transactiontype: String,
    exchange: String,
    ordertype: String,
    producttype: String,
    duration: String,
    quantity: String,
    price: String,
    triggerprice: String,
}

#[derive(Debug, Deserialize)]
pub struct OrderResponse {
    pub orderid: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderStatus {
    pub orderid: Option<String>,
    pub tradingsymbol: Option<String>,
    pub transactiontype: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    pub quantity: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    pub price: Option<String>,
    pub status: Option<String>,
    pub text: Option<String>,
}

/// Angel One returns some numeric fields as JSON numbers in one endpoint and
/// as strings in another; accept both.
fn string_or_number<'de, D>(d: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

#[derive(Debug, Deserialize)]
struct SearchScrip {
    pub symboltoken: Option<String>,
    pub tradingsymbol: Option<String>,
    pub exchange: Option<String>,
}

/// A broker instrument resolved from a user-typed symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct Instrument {
    pub tradingsymbol: String,
    pub token: String,
    pub exchange: String,
}

/// Pick the cash-equity (`-EQ`) instrument for `symbol` from searchScrip
/// results. Only an exact match is accepted, so a typo or partial name can
/// never resolve to a different stock or series.
fn pick_equity(results: Vec<SearchScrip>, symbol: &str, exchange: &str) -> Result<Instrument> {
    let wanted = format!("{}-EQ", symbol.trim_end_matches("-EQ"));
    let mut matches = results.into_iter().filter(|s| {
        s.tradingsymbol.as_deref() == Some(wanted.as_str())
            && s.exchange.as_deref().is_none_or(|e| e == exchange)
    });
    let Some(hit) = matches.next() else {
        bail!(
            "No exact {} equity match for {} on {}. Check the symbol with `stockwise search`.",
            wanted,
            symbol,
            exchange
        );
    };
    if matches.next().is_some() {
        bail!(
            "Several instruments match {} on {}; refusing to guess.",
            wanted,
            exchange
        );
    }
    Ok(Instrument {
        tradingsymbol: wanted,
        token: hit.symboltoken.context("Broker returned no symbol token")?,
        exchange: exchange.to_string(),
    })
}

/// Unwrap an Angel One response envelope, turning `status: false` into an
/// error instead of an empty result.
fn expect_ok<T>(resp: ApiResponse<T>, what: &str) -> Result<Option<T>> {
    if !resp.status {
        bail!("Angel One {} failed: {}", what, resp.message);
    }
    Ok(resp.data)
}

impl AngelClient {
    pub async fn new() -> Result<Self> {
        let config = AngelConfig::load()?;
        if !config.is_configured() {
            bail!(
                "Angel One is not configured. Run `stockwise trade setup` for instructions, then edit {}",
                crate::store::path(CONFIG_FILE)?.display()
            );
        }

        let totp = generate_totp(&config.totp_secret)
            .context("Failed to generate TOTP. Check your TOTP secret.")?;

        let client = reqwest::Client::builder()
            .user_agent("stockwise-cli")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;

        // Login
        let login_body = serde_json::json!({
            "clientcode": config.client_id,
            "password": config.password,
            "totp": totp
        });

        let resp: ApiResponse<LoginData> = client
            .post(format!(
                "{}/rest/auth/angelbroking/user/v1/loginByPassword",
                BASE_URL
            ))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("X-UserType", "USER")
            .header("X-SourceID", "WEB")
            .header("X-ClientLocalIP", "127.0.0.1")
            .header("X-ClientPublicIP", "127.0.0.1")
            .header("X-MACAddress", "00:00:00:00:00:00")
            .header("X-PrivateKey", &config.api_key)
            .json(&login_body)
            .send()
            .await
            .context("Failed to connect to Angel One API")?
            .json()
            .await
            .context("Failed to parse login response")?;

        if !resp.status {
            bail!("Angel One login failed: {}", resp.message);
        }

        let data = resp.data.context("No login data returned")?;

        Ok(Self {
            client,
            config,
            jwt_token: data.jwt_token,
            refresh_token: data.refresh_token,
        })
    }

    fn auth_headers(&self) -> Result<reqwest::header::HeaderMap> {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", self.jwt_token))
                .context("Broker returned an invalid session token")?,
        );
        for (name, value) in [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
            ("X-UserType", "USER"),
            ("X-SourceID", "WEB"),
            ("X-ClientLocalIP", "127.0.0.1"),
            ("X-ClientPublicIP", "127.0.0.1"),
            ("X-MACAddress", "00:00:00:00:00:00"),
        ] {
            headers.insert(name, HeaderValue::from_static(value));
        }
        headers.insert(
            "X-PrivateKey",
            HeaderValue::from_str(&self.config.api_key)
                .context("API key in the Angel One config contains invalid characters")?,
        );
        Ok(headers)
    }

    pub async fn get_holdings(&self) -> Result<Vec<Holding>> {
        let resp: ApiResponse<Vec<Holding>> = self
            .client
            .get(format!(
                "{}/rest/secure/angelbroking/portfolio/v1/getHolding",
                BASE_URL
            ))
            .headers(self.auth_headers()?)
            .send()
            .await?
            .json()
            .await?;

        Ok(expect_ok(resp, "holdings request")?.unwrap_or_default())
    }

    pub async fn get_positions(&self) -> Result<Vec<Position>> {
        let resp: ApiResponse<Vec<Position>> = self
            .client
            .get(format!(
                "{}/rest/secure/angelbroking/order/v1/getPosition",
                BASE_URL
            ))
            .headers(self.auth_headers()?)
            .send()
            .await?
            .json()
            .await?;

        Ok(expect_ok(resp, "positions request")?.unwrap_or_default())
    }

    /// Resolve a user-typed NSE/BSE symbol (e.g. `RELIANCE`) to its exact
    /// cash-equity instrument.
    pub async fn resolve_equity(&self, symbol: &str, exchange: &str) -> Result<Instrument> {
        let body = serde_json::json!({
            "exchange": exchange,
            "searchscrip": symbol
        });
        let resp: ApiResponse<Vec<SearchScrip>> = self
            .client
            .post(format!(
                "{}/rest/secure/angelbroking/order/v1/searchScrip",
                BASE_URL
            ))
            .headers(self.auth_headers()?)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        let results = expect_ok(resp, "symbol search")?.unwrap_or_default();
        pick_equity(results, symbol, exchange)
    }

    pub async fn place_order(
        &self,
        instrument: &Instrument,
        transaction: &str, // BUY or SELL
        qty: u32,
        order_type: &str, // MARKET, LIMIT, SL
        price: f64,
        trigger: f64,
    ) -> Result<OrderResponse> {
        let order = OrderRequest {
            variety: "NORMAL".into(),
            tradingsymbol: instrument.tradingsymbol.clone(),
            symboltoken: instrument.token.clone(),
            transactiontype: transaction.into(),
            exchange: instrument.exchange.clone(),
            ordertype: order_type.into(),
            producttype: "DELIVERY".into(),
            duration: "DAY".into(),
            quantity: qty.to_string(),
            price: if order_type == "MARKET" {
                "0".into()
            } else {
                format!("{:.2}", price)
            },
            // Angel One only accepts a trigger price on stop-loss order types;
            // sending it on MARKET/LIMIT orders is rejected.
            triggerprice: if matches!(order_type, "SL" | "SL-M") && trigger > 0.0 {
                format!("{:.2}", trigger)
            } else {
                "0".into()
            },
        };

        let resp: ApiResponse<OrderResponse> = self
            .client
            .post(format!(
                "{}/rest/secure/angelbroking/order/v1/placeOrder",
                BASE_URL
            ))
            .headers(self.auth_headers()?)
            .json(&order)
            .send()
            .await
            .context(
                "Could not reach Angel One while placing the order. Its state is UNKNOWN — \
                 check the Angel One app or `stockwise trade orders` before retrying",
            )?
            .json()
            .await
            .context(
                "Unreadable response while placing the order. Its state is UNKNOWN — \
                 check the Angel One app or `stockwise trade orders` before retrying",
            )?;

        if !resp.status {
            bail!("Order failed: {}", resp.message);
        }

        resp.data.context("No order response data")
    }

    pub async fn get_order_book(&self) -> Result<Vec<OrderStatus>> {
        let resp: ApiResponse<Vec<OrderStatus>> = self
            .client
            .get(format!(
                "{}/rest/secure/angelbroking/order/v1/getOrderBook",
                BASE_URL
            ))
            .headers(self.auth_headers()?)
            .send()
            .await?
            .json()
            .await?;

        Ok(expect_ok(resp, "order book request")?.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrip(sym: &str, token: &str) -> SearchScrip {
        SearchScrip {
            symboltoken: Some(token.into()),
            tradingsymbol: Some(sym.into()),
            exchange: Some("NSE".into()),
        }
    }

    #[test]
    fn pick_equity_takes_exact_eq_series_not_first_hit() {
        let results = vec![
            scrip("RELIANCE-BE", "1"),
            scrip("RELIANCEPP-EQ", "2"),
            scrip("RELIANCE-EQ", "2885"),
        ];
        let inst = pick_equity(results, "reliance".to_uppercase().as_str(), "NSE").unwrap();
        assert_eq!(inst.tradingsymbol, "RELIANCE-EQ");
        assert_eq!(inst.token, "2885");
    }

    #[test]
    fn pick_equity_rejects_partial_matches() {
        let results = vec![scrip("TATAMOTORS-EQ", "1"), scrip("TATASTEEL-EQ", "2")];
        assert!(pick_equity(results, "TATA", "NSE").is_err());
    }

    #[test]
    fn placeholder_config_is_not_configured() {
        assert!(!AngelConfig::template().is_configured());
        assert!(!AngelConfig::default().is_configured());
    }

    #[test]
    fn order_status_accepts_numeric_price() {
        let o: OrderStatus =
            serde_json::from_str(r#"{"orderid":"1","price":194.5,"quantity":"3"}"#).unwrap();
        assert_eq!(o.price.as_deref(), Some("194.5"));
        assert_eq!(o.quantity.as_deref(), Some("3"));
    }
}
