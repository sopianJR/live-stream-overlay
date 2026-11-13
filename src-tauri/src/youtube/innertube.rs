//! Klien HTTP untuk InnerTube (API internal web YouTube). Tidak memakai login maupun kuota
//! YouTube Data API. Tidak resmi: bisa berubah sewaktu-waktu, jadi semua penguraian ada di
//! `parse.rs` yang teruji dengan fixture.

use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_LANGUAGE, COOKIE, ORIGIN, REFERER};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

use super::parse::{self, ChatPage, LivePage, PageError, VideoInfo, Viewership};

const BASE: &str = "https://www.youtube.com";
/// Dipakai sampai versi terbaru terbaca dari halaman YouTube.
const FALLBACK_CLIENT_VERSION: &str = "2.20261112.00.00";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Jaringan putus, timeout, atau server YouTube bermasalah: layak dicoba ulang.
    Network(String),
    /// 404: channel atau video tidak ada.
    NotFound,
    /// Ditolak YouTube (4xx lain), mis. token chat kedaluwarsa.
    Rejected(u16),
    /// Respons tidak bisa dibaca.
    Invalid(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(e) => write!(f, "tidak bisa menghubungi YouTube ({e})"),
            Self::NotFound => f.write_str("tidak ditemukan di YouTube"),
            Self::Rejected(code) => write!(f, "ditolak YouTube (HTTP {code})"),
            Self::Invalid(e) => write!(f, "respons YouTube tidak dikenali ({e})"),
        }
    }
}

fn network(e: reqwest::Error) -> FetchError {
    // Tampilkan jenis kesalahan tanpa URL lengkap (berisi token yang panjang).
    let kind = if e.is_timeout() {
        "waktu habis"
    } else if e.is_connect() {
        "gagal tersambung"
    } else if e.is_decode() {
        return FetchError::Invalid("isi respons rusak".into());
    } else {
        "kesalahan jaringan"
    };
    FetchError::Network(kind.into())
}

fn check(status: StatusCode) -> Result<(), FetchError> {
    match status {
        s if s.is_success() => Ok(()),
        StatusCode::NOT_FOUND => Err(FetchError::NotFound),
        s if s.is_server_error() || s == StatusCode::TOO_MANY_REQUESTS => {
            Err(FetchError::Network(format!("HTTP {}", s.as_u16())))
        }
        s => Err(FetchError::Rejected(s.as_u16())),
    }
}

pub struct Innertube {
    http: Client,
    client_version: Mutex<String>,
}

impl Innertube {
    pub fn new() -> Result<Self, String> {
        let mut headers = HeaderMap::new();
        // Bahasa Indonesia untuk teks dari YouTube (mis. "Selamat datang di …").
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("id-ID,id;q=0.9,en;q=0.8"));
        // Lewati halaman persetujuan cookie (consent.youtube.com) di beberapa wilayah.
        headers.insert(COOKIE, HeaderValue::from_static("SOCS=CAI"));
        headers.insert(ORIGIN, HeaderValue::from_static(BASE));
        headers.insert(REFERER, HeaderValue::from_static("https://www.youtube.com/"));
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .timeout(TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .gzip(true)
            .build()
            .map_err(|e| format!("gagal menyiapkan klien HTTP: {e}"))?;
        Ok(Self { http, client_version: Mutex::new(FALLBACK_CLIENT_VERSION.into()) })
    }

    fn version(&self) -> String {
        self.client_version.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn context(&self) -> Value {
        json!({ "client": { "clientName": "WEB", "clientVersion": self.version(), "hl": "id", "gl": "ID" } })
    }

    async fn post(&self, endpoint: &str, mut body: Value) -> Result<Value, FetchError> {
        body["context"] = self.context();
        let response = self
            .http
            .post(format!("{BASE}/youtubei/v1/{endpoint}?prettyPrint=false"))
            .header("X-Youtube-Client-Name", "1")
            .header("X-Youtube-Client-Version", self.version())
            .json(&body)
            .send()
            .await
            .map_err(network)?;
        check(response.status())?;
        response.json::<Value>().await.map_err(network)
    }

    /// Halaman `/<channel>/live`: mengarah ke live yang sedang berjalan (atau yang dijadwalkan).
    pub async fn live_page(&self, channel_path: &str) -> Result<LivePage, FetchError> {
        let response = self
            .http
            .get(format!("{BASE}/{channel_path}/live"))
            .send()
            .await
            .map_err(network)?;
        check(response.status())?;
        let html = response.text().await.map_err(network)?;
        let page = parse::parse_live_page(&html);
        if let Some(version) = &page.client_version {
            *self.client_version.lock().unwrap_or_else(|e| e.into_inner()) = version.clone();
        }
        Ok(page)
    }

    pub async fn video(&self, video_id: &str) -> Result<VideoInfo, FetchError> {
        let v = self.post("next", json!({ "videoId": video_id })).await?;
        if v.get("contents").is_none() {
            return Err(FetchError::NotFound);
        }
        Ok(parse::parse_next(&v))
    }

    /// `Ok(None)` = tidak ada chat lagi (live selesai atau token tidak berlaku).
    pub async fn chat_page(&self, continuation: &str) -> Result<Option<ChatPage>, FetchError> {
        let v = self.post("live_chat/get_live_chat", json!({ "continuation": continuation })).await?;
        match parse::parse_chat_page(&v) {
            Ok(page) => Ok(Some(page)),
            Err(PageError::NoChat) => Ok(None),
        }
    }

    pub async fn viewership(&self, video_id: &str) -> Result<Option<Viewership>, FetchError> {
        let v = self.post("updated_metadata", json!({ "videoId": video_id })).await?;
        Ok(parse::parse_viewership(&v))
    }
}
