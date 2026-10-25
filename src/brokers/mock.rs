//! A tiny local HTTP server for broker tests: canned responses per route,
//! and a record of every request so tests can assert what was (and was not)
//! sent. Never talks to the internet.

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Route {
    pub method: &'static str,
    /// Path without the query string.
    pub path: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: &'static str,
}

impl Route {
    pub fn json(method: &'static str, path: &str, body: &str) -> Self {
        Self {
            method,
            path: path.into(),
            status: 200,
            body: body.as_bytes().to_vec(),
            content_type: "application/json",
        }
    }

    pub fn bytes(
        method: &'static str,
        path: &str,
        body: Vec<u8>,
        content_type: &'static str,
    ) -> Self {
        Self {
            method,
            path: path.into(),
            status: 200,
            body,
            content_type,
        }
    }

    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// Path including the query string.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }

    /// Value of a form-encoded body field.
    pub fn form(&self, key: &str) -> Option<String> {
        super::http::query_param(&format!("?{}", self.body), key)
    }
}

pub struct MockServer {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl MockServer {
    pub async fn start(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let log = log.clone();
                let routes = routes.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else {
                        return;
                    };
                    let route = routes
                        .iter()
                        .find(|r| r.method == req.method && r.path == req.path());
                    log.lock().unwrap().push(req);
                    let (status, body, ct) = match route {
                        Some(r) => (r.status, r.body.clone(), r.content_type),
                        None => (
                            404,
                            b"{\"error\":\"no mock route\"}".to_vec(),
                            "application/json",
                        ),
                    };
                    let head = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&body).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        Self { url, requests }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    /// How many requests hit `method path`.
    pub fn count(&self, method: &str, path: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.method == method && r.path() == path)
            .count()
    }

    pub fn last(&self, method: &str, path: &str) -> Request {
        self.requests()
            .into_iter()
            .rev()
            .find(|r| r.method == method && r.path() == path)
            .unwrap_or_else(|| panic!("no {method} {path} request"))
    }
}

/// Fetch every endpoint in `fixtures` through `broker.rows` and assert the
/// rows match the broker's documented shape: every field StockWise reads is
/// present, and no field is missing from the documented list.
pub async fn assert_fixture_shapes(broker: &dyn super::Broker, fixtures: &[super::Endpoint]) {
    for &ep in fixtures {
        let rows = broker
            .rows(ep)
            .await
            .unwrap_or_else(|e| panic!("{ep:?}: {e:#}"));
        let shape = broker.shape(ep).expect("a shape for every fixture");
        let report = super::check_shape(&rows, &shape);
        assert!(report.rows > 0, "{ep:?}: no rows");
        assert!(
            report.missing.is_empty() && report.unknown.is_empty() && report.not_objects == 0,
            "{ep:?}: {report:?}"
        );
    }
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[header_end..].to_vec();
    while body.len() < len {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some(Request {
        method,
        target,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}
