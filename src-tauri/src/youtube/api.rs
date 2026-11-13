//! YouTube Data API v3 (resmi, butuh login). Dipakai hanya untuk mengirim pesan live chat:
//! membaca chat tetap lewat InnerTube agar tidak menghabiskan kuota.
//!
//! Biaya kuota (default 10.000 unit/hari): `channels.list` & `videos.list` 1 unit,
//! `liveChatMessages.insert` 50 unit (±190 pesan/hari).

use std::fmt;

use reqwest::Client;
use serde_json::{json, Value};

use super::oauth::Endpoints;

pub const MAX_MESSAGE_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Token akses kedaluwarsa / tidak valid: perlu refresh.
    Unauthorized,
    Quota,
    RateLimited,
    ChatEnded,
    ChatDisabled,
    /// Live chat tidak ditemukan / tidak aktif untuk video ini.
    NoActiveChat,
    /// YouTube Data API v3 belum diaktifkan di project Google Cloud.
    ApiDisabled,
    NotAllowed(String),
    Network(String),
    Other(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized => f.write_str("Sesi login Google kedaluwarsa. Login ulang."),
            Self::Quota => f.write_str(
                "Kuota YouTube API hari ini habis (±190 pesan/hari). Kuota pulih tengah malam waktu Pasifik.",
            ),
            Self::RateLimited => f.write_str("Terlalu cepat mengirim pesan. Tunggu sebentar."),
            Self::ChatEnded => f.write_str("Live chat sudah berakhir."),
            Self::ChatDisabled => f.write_str("Live chat dimatikan untuk live ini."),
            Self::NoActiveChat => f.write_str("Live chat tidak aktif untuk video ini (belum live atau sudah selesai)."),
            Self::ApiDisabled => f.write_str(
                "YouTube Data API v3 belum diaktifkan di project Google Cloud Anda. Aktifkan di \
                 APIs & Services → Library.",
            ),
            Self::NotAllowed(m) => write!(f, "Tidak diizinkan: {m}"),
            Self::Network(e) => write!(f, "Tidak bisa menghubungi YouTube ({e})."),
            Self::Other(e) => write!(f, "YouTube menolak: {e}"),
        }
    }
}

/// Memetakan respons error Google API ke pesan yang bisa dipahami user.
pub fn classify(status: u16, body: &str) -> ApiError {
    let v: Value = serde_json::from_str(body).unwrap_or_default();
    let err = &v["error"];
    let message = err["message"].as_str().unwrap_or_default().to_string();
    let reasons: Vec<&str> = err["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["reason"].as_str())
        .chain(
            err["details"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d["reason"].as_str()),
        )
        .collect();
    let has = |r: &str| reasons.contains(&r);

    if status == 401 {
        ApiError::Unauthorized
    } else if has("quotaExceeded") || has("dailyLimitExceeded") {
        ApiError::Quota
    } else if has("rateLimitExceeded") || has("userRateLimitExceeded") || status == 429 {
        ApiError::RateLimited
    } else if has("liveChatEnded") {
        ApiError::ChatEnded
    } else if has("liveChatDisabled") {
        ApiError::ChatDisabled
    } else if has("liveChatNotFound") || has("videoNotFound") {
        ApiError::NoActiveChat
    } else if has("accessNotConfigured") || has("SERVICE_DISABLED") {
        ApiError::ApiDisabled
    } else if status == 403 {
        ApiError::NotAllowed(if message.is_empty() { "akun ini tidak boleh mengirim chat di sini".into() } else { message })
    } else if status >= 500 {
        ApiError::Network(format!("HTTP {status}"))
    } else {
        ApiError::Other(if message.is_empty() { format!("HTTP {status}") } else { message })
    }
}

/// Merapikan dan memeriksa pesan sebelum mengirim (gratis, tanpa memanggil API).
pub fn validate_message(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Pesan kosong.".into());
    }
    if text.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!("Pesan maksimal {MAX_MESSAGE_CHARS} karakter."));
    }
    Ok(text.to_string())
}

async fn send(request: reqwest::RequestBuilder) -> Result<Value, ApiError> {
    let response = request.send().await.map_err(|e| {
        ApiError::Network(if e.is_timeout() { "waktu habis".into() } else { "gagal tersambung".into() })
    })?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|_| ApiError::Network("respons terputus".into()))?;
    if (200..300).contains(&status) {
        Ok(serde_json::from_str(&body).unwrap_or_default())
    } else {
        Err(classify(status, &body))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub title: String,
}

/// Channel milik akun yang login (1 unit kuota).
pub async fn my_channel(http: &Client, ep: &Endpoints, token: &str) -> Result<Channel, ApiError> {
    let v = send(
        http.get(format!("{}/channels", ep.api))
            .query(&[("part", "snippet"), ("mine", "true")])
            .bearer_auth(token),
    )
    .await?;
    let item = &v["items"][0];
    match (item["id"].as_str(), item["snippet"]["title"].as_str()) {
        (Some(id), Some(title)) => Ok(Channel { id: id.into(), title: title.into() }),
        _ => Err(ApiError::NotAllowed("akun Google ini belum punya channel YouTube".into())),
    }
}

/// ID live chat aktif dari sebuah video (1 unit kuota).
pub async fn live_chat_id(http: &Client, ep: &Endpoints, token: &str, video_id: &str) -> Result<String, ApiError> {
    let v = send(
        http.get(format!("{}/videos", ep.api))
            .query(&[("part", "liveStreamingDetails"), ("id", video_id)])
            .bearer_auth(token),
    )
    .await?;
    v["items"][0]["liveStreamingDetails"]["activeLiveChatId"]
        .as_str()
        .map(str::to_string)
        .ok_or(ApiError::NoActiveChat)
}

/// Mengirim pesan ke live chat (50 unit kuota).
pub async fn send_message(
    http: &Client,
    ep: &Endpoints,
    token: &str,
    live_chat_id: &str,
    text: &str,
) -> Result<(), ApiError> {
    let body = json!({
        "snippet": {
            "liveChatId": live_chat_id,
            "type": "textMessageEvent",
            "textMessageDetails": { "messageText": text },
        }
    });
    send(
        http.post(format!("{}/liveChatMessages", ep.api))
            .query(&[("part", "snippet")])
            .bearer_auth(token)
            .json(&body),
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use crate::youtube::mock_http::{MockHttp, Response};
    use super::*;

    fn error_body(status: u16, reason: &str) -> String {
        json!({ "error": { "code": status, "message": format!("pesan {reason}"), "errors": [{ "reason": reason, "domain": "youtube.liveChat" }] } })
            .to_string()
    }

    #[test]
    fn error_mapping() {
        assert_eq!(classify(401, "{}"), ApiError::Unauthorized);
        assert_eq!(classify(403, &error_body(403, "quotaExceeded")), ApiError::Quota);
        assert_eq!(classify(403, &error_body(403, "liveChatEnded")), ApiError::ChatEnded);
        assert_eq!(classify(403, &error_body(403, "liveChatDisabled")), ApiError::ChatDisabled);
        assert_eq!(classify(404, &error_body(404, "liveChatNotFound")), ApiError::NoActiveChat);
        assert_eq!(classify(403, &error_body(403, "rateLimitExceeded")), ApiError::RateLimited);
        assert_eq!(classify(403, &error_body(403, "forbidden")), ApiError::NotAllowed("pesan forbidden".into()));
        assert_eq!(classify(503, "gangguan"), ApiError::Network("HTTP 503".into()));

        // Format error baru Google untuk API yang belum diaktifkan.
        let disabled = json!({ "error": { "code": 403, "message": "YouTube Data API v3 has not been used in project 1 before",
            "status": "PERMISSION_DENIED", "details": [{ "@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "SERVICE_DISABLED" }] } });
        assert_eq!(classify(403, &disabled.to_string()), ApiError::ApiDisabled);
    }

    #[test]
    fn message_validation() {
        assert_eq!(validate_message("  halo  "), Ok("halo".into()));
        assert!(validate_message("   ").is_err());
        assert!(validate_message(&"a".repeat(200)).is_ok());
        assert!(validate_message(&"a".repeat(201)).unwrap_err().contains("200"));
        // Dihitung per karakter, bukan per byte.
        assert!(validate_message(&"é".repeat(200)).is_ok());
    }

    #[tokio::test]
    async fn calls_send_bearer_token_and_parse_responses() {
        let mock = MockHttp::start(|req| {
            if req.header("authorization") != Some("Bearer AKSES") {
                return Response::json(401, json!({ "error": { "code": 401, "message": "Invalid Credentials" } }));
            }
            match (req.method.as_str(), req.path.as_str()) {
                ("GET", "/api/channels") => Response::json(
                    200,
                    json!({ "items": [{ "id": "UCsaya", "snippet": { "title": "Kanal Saya" } }] }),
                ),
                ("GET", "/api/videos") if req.query("id") == Some("LIVE1234567") => Response::json(
                    200,
                    json!({ "items": [{ "liveStreamingDetails": { "activeLiveChatId": "CHAT-1" } }] }),
                ),
                ("GET", "/api/videos") => Response::json(200, json!({ "items": [{ "liveStreamingDetails": {} }] })),
                ("POST", "/api/liveChatMessages") => {
                    let body: Value = serde_json::from_str(&req.body).unwrap();
                    if body["snippet"]["liveChatId"] == "CHAT-1" {
                        Response::json(200, json!({ "id": "MSG" }))
                    } else {
                        Response::json(403, serde_json::from_str(&error_body(403, "liveChatEnded")).unwrap())
                    }
                }
                _ => Response::json(404, json!({})),
            }
        })
        .await;
        let ep = Endpoints { api: format!("{}/api", mock.base), ..Endpoints::default() };
        let http = Client::new();

        let channel = my_channel(&http, &ep, "AKSES").await.unwrap();
        assert_eq!(channel, Channel { id: "UCsaya".into(), title: "Kanal Saya".into() });
        assert_eq!(my_channel(&http, &ep, "SALAH").await.unwrap_err(), ApiError::Unauthorized);

        assert_eq!(live_chat_id(&http, &ep, "AKSES", "LIVE1234567").await.unwrap(), "CHAT-1");
        assert_eq!(live_chat_id(&http, &ep, "AKSES", "LAINLAIN123").await.unwrap_err(), ApiError::NoActiveChat);

        send_message(&http, &ep, "AKSES", "CHAT-1", "halo semua").await.unwrap();
        assert_eq!(send_message(&http, &ep, "AKSES", "CHAT-LAMA", "x").await.unwrap_err(), ApiError::ChatEnded);

        let sent = mock.requests().into_iter().find(|r| r.method == "POST").unwrap();
        let body: Value = serde_json::from_str(&sent.body).unwrap();
        assert_eq!(body["snippet"]["textMessageDetails"]["messageText"], "halo semua");
        assert_eq!(sent.query("part"), Some("snippet"));
    }
}
