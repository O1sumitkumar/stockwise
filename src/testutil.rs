//! A minimal HTTP/1.1 mock server for tests of the cloud-AI and data-provider
//! clients, so no test ever reaches a real API.
//!
//! Each accepted connection is answered with the next canned response (the
//! last one repeats) and the request is recorded for assertions.

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    /// Path plus query string, as sent.
    pub target: String,
    /// Header names are lower-cased.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

pub struct MockServer {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    /// Serve `(status, body)` pairs in order; the last one repeats.
    pub async fn start(responses: Vec<(u16, String)>) -> Self {
        assert!(!responses.is_empty());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        tokio::spawn(async move {
            let mut served = 0usize;
            while let Ok((mut stream, _)) = listener.accept().await {
                let Some(req) = read_request(&mut stream).await else {
                    continue;
                };
                log.lock().unwrap().push(req);
                let (status, body) = &responses[served.min(responses.len() - 1)];
                served += 1;
                let reply = format!(
                    "HTTP/1.1 {} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        Self { url, requests }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let target = request_line.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).to_string();
    Some(Recorded {
        method,
        target,
        headers,
        body,
    })
}
