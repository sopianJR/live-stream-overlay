//! Login Google untuk aplikasi desktop (OAuth 2.0 "installed app"):
//! PKCE + redirect ke `http://127.0.0.1:<port>` yang didengarkan aplikasi ini sebentar.
//! Hanya meminta scope `youtube.force-ssl`, yang dibutuhkan untuk mengirim pesan live chat.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use url::Url;

pub const SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";
/// Batas waktu menyelesaikan login di browser.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Alamat layanan Google; diganti server tiruan saat test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
    pub api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
            api: "https://www.googleapis.com/youtube/v3".into(),
        }
    }
}

/// OAuth client tipe "Desktop app" milik user (dibuat di Google Cloud Console).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
}

impl ClientCredentials {
    pub fn validated(client_id: &str, client_secret: &str) -> Result<Self, String> {
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        if !client_id.ends_with(".apps.googleusercontent.com") || client_id.contains(char::is_whitespace) {
            return Err("Client ID tidak valid. Bentuknya seperti 1234-abcd.apps.googleusercontent.com.".into());
        }
        if client_secret.is_empty() || client_secret.contains(char::is_whitespace) {
            return Err("Client secret kosong atau tidak valid.".into());
        }
        Ok(Self { client_id: client_id.into(), client_secret: client_secret.into() })
    }
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// String acak aman untuk URL (`bytes` byte acak, di-encode base64url).
pub fn random_token(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| format!("gagal membuat angka acak: {e}"))?;
    Ok(b64url(&buf))
}

#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn challenge_for(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

pub fn pkce() -> Result<Pkce, String> {
    // 32 byte → 43 karakter, panjang minimum yang diizinkan RFC 7636.
    let verifier = random_token(32)?;
    let challenge = challenge_for(&verifier);
    Ok(Pkce { verifier, challenge })
}

pub fn auth_url(ep: &Endpoints, client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    let params = [
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        // Minta refresh token agar tidak perlu login setiap kali aplikasi dibuka.
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    match Url::parse_with_params(&ep.auth, &params) {
        Ok(url) => url.into(),
        Err(_) => ep.auth.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginError {
    Denied,
    Timeout,
    Cancelled,
    Browser(String),
    Io(String),
    Google(String),
    Token(TokenFailure),
}

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied => f.write_str("Login dibatalkan di halaman Google."),
            Self::Timeout => f.write_str("Login tidak diselesaikan dalam 5 menit."),
            Self::Cancelled => f.write_str("Login dibatalkan."),
            Self::Browser(e) => write!(f, "Gagal membuka browser: {e}"),
            Self::Io(e) => write!(f, "Gagal menerima jawaban login: {e}"),
            Self::Google(e) => write!(f, "Google menolak login: {e}"),
            Self::Token(e) => fmt::Display::fmt(e, f),
        }
    }
}

/// Hasil membaca satu request ke redirect URI.
#[derive(Debug, PartialEq, Eq)]
pub enum Redirect {
    Code(String),
    Error(LoginError),
    /// `state` tidak cocok: bukan dari login yang sedang berjalan, abaikan.
    Foreign,
    /// Bukan request login (mis. `/favicon.ico`).
    Other,
}

/// `target` = path + query dari baris request HTTP, mis. `/?code=…&state=…`.
pub fn parse_redirect(target: &str, state: &str) -> Redirect {
    let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else { return Redirect::Other };
    if url.path() != "/" {
        return Redirect::Other;
    }
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    if q.get("state").map(String::as_str) != Some(state) {
        return if q.contains_key("code") || q.contains_key("error") { Redirect::Foreign } else { Redirect::Other };
    }
    match (q.get("code"), q.get("error")) {
        (Some(code), _) if !code.is_empty() => Redirect::Code(code.clone()),
        (_, Some(e)) if e == "access_denied" => Redirect::Error(LoginError::Denied),
        (_, Some(e)) => Redirect::Error(LoginError::Google(e.clone())),
        _ => Redirect::Other,
    }
}

fn page(title: &str, text: &str) -> String {
    format!(
        "<!doctype html><html lang=\"id\"><meta charset=\"utf-8\"><title>LiveStream Overlay</title>\
         <body style=\"font-family:Segoe UI,system-ui,sans-serif;background:#111116;color:#ececf2;\
         display:grid;place-items:center;height:100vh;margin:0\"><div style=\"text-align:center\">\
         <h2>{title}</h2><p>{text}</p></div></body></html>"
    )
}

async fn read_target(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await.ok()?.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf);
    let mut parts = head.lines().next()?.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().map(str::to_string)).flatten()
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Menunggu browser kembali ke redirect URI dan mengembalikan `code`.
pub async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String, LoginError> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| LoginError::Io(e.to_string()))?;
        let Some(target) = read_target(&mut stream).await else { continue };
        match parse_redirect(&target, state) {
            Redirect::Code(code) => {
                respond(
                    &mut stream,
                    "200 OK",
                    &page("Login berhasil ✓", "Tab ini boleh ditutup. Kembali ke LiveStream Overlay."),
                )
                .await;
                return Ok(code);
            }
            Redirect::Error(e) => {
                respond(&mut stream, "200 OK", &page("Login gagal", &e.to_string())).await;
                return Err(e);
            }
            Redirect::Foreign => {
                respond(&mut stream, "400 Bad Request", &page("Permintaan tidak dikenal", "Ulangi login dari aplikasi.")).await;
            }
            Redirect::Other => respond(&mut stream, "404 Not Found", "").await,
        }
    }
}

// ---------- Token ----------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenFailure {
    /// `invalid_grant`: refresh token dicabut atau kedaluwarsa (mode Testing: 7 hari).
    Revoked,
    /// Client ID / secret salah atau OAuth client dihapus.
    InvalidClient,
    Network(String),
    Other(String),
}

impl fmt::Display for TokenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Revoked => f.write_str(
                "Sesi login Google sudah tidak berlaku (dicabut, atau lewat 7 hari pada mode Testing). Login ulang.",
            ),
            Self::InvalidClient => f.write_str("Client ID atau client secret tidak cocok. Periksa di Google Cloud Console."),
            Self::Network(e) => write!(f, "Tidak bisa menghubungi Google ({e})."),
            Self::Other(e) => write!(f, "Google menolak token: {e}"),
        }
    }
}

#[derive(Deserialize)]
struct TokenErrorBody {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

pub fn classify_token_error(status: u16, body: &str) -> TokenFailure {
    match serde_json::from_str::<TokenErrorBody>(body) {
        Ok(e) if e.error == "invalid_grant" => TokenFailure::Revoked,
        Ok(e) if e.error == "invalid_client" || e.error == "unauthorized_client" => TokenFailure::InvalidClient,
        Ok(e) => TokenFailure::Other(e.error_description.unwrap_or(e.error)),
        Err(_) if status >= 500 => TokenFailure::Network(format!("HTTP {status}")),
        Err(_) => TokenFailure::Other(format!("HTTP {status}")),
    }
}

async fn token_request(http: &Client, ep: &Endpoints, params: &[(&str, &str)]) -> Result<Tokens, TokenFailure> {
    let response = http
        .post(&ep.token)
        .form(params)
        .send()
        .await
        .map_err(|e| TokenFailure::Network(if e.is_timeout() { "waktu habis".into() } else { "gagal tersambung".into() }))?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|_| TokenFailure::Network("respons terputus".into()))?;
    if (200..300).contains(&status) {
        serde_json::from_str(&body).map_err(|_| TokenFailure::Other("respons token tidak dikenali".into()))
    } else {
        Err(classify_token_error(status, &body))
    }
}

pub async fn exchange_code(
    http: &Client,
    ep: &Endpoints,
    client: &ClientCredentials,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Tokens, TokenFailure> {
    token_request(
        http,
        ep,
        &[
            ("code", code),
            ("client_id", &client.client_id),
            ("client_secret", &client.client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ],
    )
    .await
}

pub async fn refresh(
    http: &Client,
    ep: &Endpoints,
    client: &ClientCredentials,
    refresh_token: &str,
) -> Result<Tokens, TokenFailure> {
    token_request(
        http,
        ep,
        &[
            ("refresh_token", refresh_token),
            ("client_id", &client.client_id),
            ("client_secret", &client.client_secret),
            ("grant_type", "refresh_token"),
        ],
    )
    .await
}

/// Mencabut izin di Google saat logout (sebisanya; gagal pun token lokal tetap dihapus).
pub async fn revoke(http: &Client, ep: &Endpoints, token: &str) {
    if let Err(e) = http.post(&ep.revoke).form(&[("token", token)]).send().await {
        log::warn!("gagal mencabut token Google: {e}");
    }
}

/// Seluruh alur login: buka browser, tunggu redirect, tukar `code` dengan token.
pub async fn authorize(
    http: &Client,
    ep: &Endpoints,
    client: &ClientCredentials,
    open_browser: impl FnOnce(&str) -> Result<(), String>,
    cancel: oneshot::Receiver<()>,
    timeout: Duration,
) -> Result<Tokens, LoginError> {
    let pkce = pkce().map_err(LoginError::Io)?;
    let state = random_token(16).map_err(LoginError::Io)?;
    // Port acak di loopback; Google mengizinkan port apa pun untuk client Desktop.
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| LoginError::Io(e.to_string()))?;
    let port = listener.local_addr().map_err(|e| LoginError::Io(e.to_string()))?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    open_browser(&auth_url(ep, &client.client_id, &redirect_uri, &pkce.challenge, &state))
        .map_err(LoginError::Browser)?;

    let code = tokio::select! {
        result = wait_for_code(&listener, &state) => result?,
        _ = cancel => return Err(LoginError::Cancelled),
        _ = tokio::time::sleep(timeout) => return Err(LoginError::Timeout),
    };
    drop(listener);
    exchange_code(http, ep, client, &code, &pkce.verifier, &redirect_uri).await.map_err(LoginError::Token)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use crate::youtube::mock_http::{MockHttp, Response};
    use super::*;

    fn client() -> ClientCredentials {
        ClientCredentials::validated("123-abc.apps.googleusercontent.com", "rahasia-uji").unwrap()
    }

    #[test]
    fn validates_client_credentials() {
        assert!(ClientCredentials::validated(" 123-abc.apps.googleusercontent.com ", " s ").is_ok());
        assert!(ClientCredentials::validated("123-abc", "s").unwrap_err().contains("Client ID"));
        assert!(ClientCredentials::validated("123-abc.apps.googleusercontent.com", "  ").unwrap_err().contains("secret"));
    }

    #[test]
    fn pkce_challenge_is_base64url_sha256() {
        // Nilai pembanding dihitung terpisah dengan hashlib + base64 Python.
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mJ92K1qUcRtjJmQhTRbM8Yu0ohQ8Oo"),
            "GPE0xypBy11dA9ZIc6tyXu1AwNJ45vLufi7IjzjqMyY"
        );
        let p = pkce().unwrap();
        assert_eq!(p.verifier.len(), 43);
        assert_ne!(p.verifier, pkce().unwrap().verifier);
    }

    #[test]
    fn auth_url_has_required_params() {
        let url = Url::parse(&auth_url(&Endpoints::default(), "cid", "http://127.0.0.1:5000", "chal", "st")).unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:5000");
        assert_eq!(q["scope"], SCOPE);
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["state"], "st");
    }

    #[test]
    fn redirect_parsing() {
        assert_eq!(parse_redirect("/?code=abc&state=s1&scope=x", "s1"), Redirect::Code("abc".into()));
        assert_eq!(parse_redirect("/?error=access_denied&state=s1", "s1"), Redirect::Error(LoginError::Denied));
        assert_eq!(parse_redirect("/?code=abc&state=lain", "s1"), Redirect::Foreign);
        assert_eq!(parse_redirect("/favicon.ico", "s1"), Redirect::Other);
        assert_eq!(parse_redirect("/", "s1"), Redirect::Other);
    }

    #[test]
    fn token_errors() {
        assert_eq!(classify_token_error(400, r#"{"error":"invalid_grant"}"#), TokenFailure::Revoked);
        assert_eq!(classify_token_error(401, r#"{"error":"invalid_client"}"#), TokenFailure::InvalidClient);
        assert_eq!(classify_token_error(503, "oops"), TokenFailure::Network("HTTP 503".into()));
    }

    /// Server tiruan Google: halaman login langsung mengarahkan kembali dengan `code`,
    /// endpoint token memeriksa PKCE.
    async fn fake_google(deny: bool) -> (MockHttp, Endpoints) {
        let challenge = Arc::new(Mutex::new(String::new()));
        let seen = Arc::clone(&challenge);
        let mock = MockHttp::start(move |req| match req.path.as_str() {
            "/auth" => {
                *seen.lock().unwrap() = req.query("code_challenge").unwrap_or_default().to_string();
                let redirect = req.query("redirect_uri").unwrap_or_default();
                let state = req.query("state").unwrap_or_default();
                if deny {
                    Response::redirect(&format!("{redirect}/?error=access_denied&state={state}"))
                } else {
                    Response::redirect(&format!("{redirect}/?code=KODE123&state={state}"))
                }
            }
            "/token" => {
                let verifier = req.form("code_verifier").unwrap_or_default();
                let ok = req.form("code").as_deref() == Some("KODE123")
                    && req.form("client_secret").as_deref() == Some("rahasia-uji")
                    && challenge_for(&verifier) == *seen.lock().unwrap();
                if ok {
                    Response::json(200, json!({ "access_token": "AKSES", "expires_in": 3599, "refresh_token": "SEGAR", "token_type": "Bearer" }))
                } else {
                    Response::json(400, json!({ "error": "invalid_grant" }))
                }
            }
            _ => Response::json(404, json!({})),
        })
        .await;
        let ep = Endpoints {
            auth: format!("{}/auth", mock.base),
            token: format!("{}/token", mock.base),
            revoke: format!("{}/revoke", mock.base),
            api: format!("{}/api", mock.base),
        };
        (mock, ep)
    }

    /// "Browser" tiruan: membuka URL login dan mengikuti redirect ke aplikasi.
    fn fake_browser(url: &str) -> Result<(), String> {
        let url = url.to_string();
        tokio::spawn(async move {
            let _ = Client::new().get(url).send().await;
        });
        Ok(())
    }

    #[tokio::test]
    async fn full_login_flow_with_pkce() {
        let (_mock, ep) = fake_google(false).await;
        let (_tx, rx) = oneshot::channel();
        let tokens = authorize(&Client::new(), &ep, &client(), fake_browser, rx, Duration::from_secs(10))
            .await
            .expect("login berhasil");
        assert_eq!(tokens.access_token, "AKSES");
        assert_eq!(tokens.refresh_token.as_deref(), Some("SEGAR"));
    }

    #[tokio::test]
    async fn denied_login() {
        let (_mock, ep) = fake_google(true).await;
        let (_tx, rx) = oneshot::channel();
        let err = authorize(&Client::new(), &ep, &client(), fake_browser, rx, Duration::from_secs(10))
            .await
            .unwrap_err();
        assert_eq!(err, LoginError::Denied);
    }

    #[tokio::test]
    async fn cancel_and_timeout() {
        let (_mock, ep) = fake_google(false).await;
        let (http, creds) = (Client::new(), client());
        let (tx, rx) = oneshot::channel();
        let never_open = |_: &str| Ok(());
        let login = authorize(&http, &ep, &creds, never_open, rx, Duration::from_secs(10));
        tx.send(()).unwrap();
        assert_eq!(login.await.unwrap_err(), LoginError::Cancelled);

        let (_tx, rx) = oneshot::channel();
        let err = authorize(&http, &ep, &creds, never_open, rx, Duration::from_millis(200))
            .await
            .unwrap_err();
        assert_eq!(err, LoginError::Timeout);
    }

    #[tokio::test]
    async fn browser_failure() {
        let (_mock, ep) = fake_google(false).await;
        let (_tx, rx) = oneshot::channel();
        let err = authorize(&Client::new(), &ep, &client(), |_| Err("tidak ada browser".into()), rx, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(matches!(err, LoginError::Browser(_)));
    }

    #[tokio::test]
    async fn refresh_token_flow() {
        let mock = MockHttp::start(|req| match req.form("refresh_token").as_deref() {
            Some("SEGAR") => Response::json(200, json!({ "access_token": "BARU", "expires_in": 3599 })),
            _ => Response::json(400, json!({ "error": "invalid_grant", "error_description": "Token has been expired or revoked." })),
        })
        .await;
        let ep = Endpoints { token: format!("{}/token", mock.base), ..Endpoints::default() };
        let ok = refresh(&Client::new(), &ep, &client(), "SEGAR").await.unwrap();
        assert_eq!(ok.access_token, "BARU");
        assert_eq!(ok.refresh_token, None);
        let err = refresh(&Client::new(), &ep, &client(), "LAMA").await.unwrap_err();
        assert_eq!(err, TokenFailure::Revoked);
        assert_eq!(mock.requests()[0].form("grant_type").as_deref(), Some("refresh_token"));
    }
}
