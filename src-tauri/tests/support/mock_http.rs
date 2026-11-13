//! Server HTTP/1.1 tiruan yang sangat sederhana untuk test (mis. menggantikan Google OAuth
//! dan YouTube Data API). Setiap koneksi menangani satu request lalu ditutup.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    /// Isi body `application/x-www-form-urlencoded`.
    pub fn form(&self, key: &str) -> Option<String> {
        url::form_urlencoded::parse(self.body.as_bytes()).find(|(k, _)| k == key).map(|(_, v)| v.into_owned())
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_string(),
        }
    }

    pub fn redirect(location: &str) -> Self {
        Self { status: 302, headers: vec![("Location".into(), location.into())], body: String::new() }
    }
}

type Handler = dyn Fn(&Request) -> Response + Send + Sync;

pub struct MockHttp {
    /// mis. `http://127.0.0.1:54321`
    pub base: String,
    requests: Arc<Mutex<Vec<Request>>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockHttp {
    pub async fn start(handler: impl Fn(&Request) -> Response + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock http");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<Handler> = Arc::new(handler);
        let log = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                let handler = Arc::clone(&handler);
                let log = Arc::clone(&log);
                conns.spawn(async move {
                    let _ = serve(stream, handler, log).await;
                });
            }
        });
        Self { base, requests, task }
    }

    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    pub fn count(&self, path: &str) -> usize {
        self.requests().iter().filter(|r| r.path == path).count()
    }
}

impl Drop for MockHttp {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Status",
    }
}

async fn serve(
    mut stream: TcpStream,
    handler: Arc<Handler>,
    log: Arc<Mutex<Vec<Request>>>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default().to_string();
    let target = first.next().unwrap_or("/").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + length {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).to_string();
    let url = url::Url::parse(&format!("http://mock{target}")).expect("target");
    let request = Request {
        method,
        path: url.path().to_string(),
        query: url.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect(),
        headers,
        body,
    };
    let response = handler(&request);
    log.lock().unwrap().push(request);

    let mut out = format!("HTTP/1.1 {} {}\r\n", response.status, reason(response.status));
    for (k, v) in &response.headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", response.body.len()));
    out.push_str(&response.body);
    stream.write_all(out.as_bytes()).await?;
    stream.shutdown().await
}
