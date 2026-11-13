//! Protokol realtime Trakteer (fungsi murni, teruji). Di-port dan dirapikan dari
//! `trakteer-live-suite` (`connection_manager`).
//!
//! - **V1**: protokol Pusher di `socket.trakteer.id`. Kunci user = Channel ID ber-base64;
//!   setelah `pusher:connection_established`, subscribe ke `creator-stream.{id}` (live) dan
//!   `creator-stream-test.{id}` (test).
//! - **V2**: WebSocket `ws.trakteer.id` dengan Stream Key `trv2-…` (dari widget alert).
//!   Donasi datang sebagai event `alert-queue:*`; yang dipakai hanya saat alert diputar.

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};

pub const PUSHER_APP_KEY: &str = "2ae25d102cc6cd41100a";
const V1_BASE: &str = "wss://socket.trakteer.id";
const V2_BASE: &str = "wss://ws.trakteer.id";
const DEFAULT_AVATAR: &str = "https://edge-cdn.trakteer.id/images/mix/default-avatar.png";
/// Dipakai jika server tidak memberi `activity_timeout`.
pub const DEFAULT_ACTIVITY_TIMEOUT_S: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// Channel ID yang sudah di-decode dari base64.
    V1Channel(String),
    /// Stream Key `trv2-…`.
    V2Token(String),
}

impl Credential {
    pub fn protocol(&self) -> Protocol {
        match self {
            Self::V1Channel(_) => Protocol::V1,
            Self::V2Token(_) => Protocol::V2,
        }
    }

    /// Alamat WebSocket. `base` hanya diganti saat test / server tiruan.
    pub fn url_with_base(&self, base: Option<&str>) -> String {
        match self {
            Self::V1Channel(_) => format!(
                "{}/app/{PUSHER_APP_KEY}?protocol=7&client=js&version=7.4.0&flash=false",
                base.unwrap_or(V1_BASE)
            ),
            Self::V2Token(token) => format!(
                "{}/ws?token={}&widget=alert-box",
                base.unwrap_or(V2_BASE),
                url::form_urlencoded::byte_serialize(token.as_bytes()).collect::<String>()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Protocol {
    V1,
    V2,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Protocol::V1 => "Channel ID (V1)",
            Protocol::V2 => "Stream Key (V2)",
        }
    }
}

const KEY_HINT: &str = "Isi Channel ID (base64) atau Stream Key (diawali trv2-) dari pengaturan widget alert di dashboard Trakteer.";

/// Mengenali kunci dari user: Stream Key V2 atau Channel ID V1 (base64).
pub fn parse_key(input: &str) -> Result<Credential, String> {
    let key = input.trim();
    if key.is_empty() {
        return Err(format!("Kunci Trakteer kosong. {KEY_HINT}"));
    }
    if key.chars().any(char::is_whitespace) {
        return Err(format!("Kunci Trakteer tidak boleh berisi spasi. {KEY_HINT}"));
    }
    if key.starts_with("trv2-") {
        return if key.len() > "trv2-".len() {
            Ok(Credential::V2Token(key.to_string()))
        } else {
            Err(format!("Stream Key tidak lengkap. {KEY_HINT}"))
        };
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(key)
        .map_err(|_| format!("Bukan kunci Trakteer yang dikenali. {KEY_HINT}"))?;
    let decoded = String::from_utf8(bytes).map_err(|_| format!("Channel ID tidak valid. {KEY_HINT}"))?;
    if decoded.is_empty() || decoded.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("Channel ID tidak valid. {KEY_HINT}"));
    }
    Ok(Credential::V1Channel(decoded))
}

// ---------- Pesan keluar (V1 / Pusher) ----------

pub fn subscribe_messages(channel: &str) -> [String; 2] {
    let sub = |name: String| json!({ "event": "pusher:subscribe", "data": { "channel": name } }).to_string();
    [sub(format!("creator-stream.{channel}")), sub(format!("creator-stream-test.{channel}"))]
}

pub fn ping_message() -> String {
    r#"{"event":"pusher:ping","data":{}}"#.to_string()
}

pub fn pong_message() -> String {
    r#"{"event":"pusher:pong","data":{}}"#.to_string()
}

// ---------- Pesan masuk ----------

/// Donasi yang sudah dinormalisasi dari payload V1 maupun V2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Donation {
    /// ID dari Trakteer (V2) untuk membuang duplikat; V1 tidak punya.
    #[serde(skip)]
    pub source_id: Option<String>,
    pub supporter_name: String,
    pub unit: String,
    pub quantity: u32,
    pub amount_rp: u64,
    /// mis. "Rp 5.000"
    pub price_text: String,
    pub message: String,
    pub avatar_url: String,
    pub unit_icon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// V1: koneksi siap; subscribe dan atur heartbeat.
    Established { activity_timeout_s: u64 },
    /// V2: Stream Key diterima.
    Authenticated,
    /// V1: server meminta pong.
    Ping,
    Error(String),
    Donation { donation: Donation, test: bool },
    Ignore,
}

/// Angka dari JSON yang bisa berupa number atau teks ("Rp 5.000", "2").
pub fn extract_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)),
        Value::String(s) => {
            let digits: String = s.chars().filter(char::is_ascii_digit).collect();
            digits.parse().ok()
        }
        _ => None,
    }
}

/// 1500000 → "Rp 1.500.000"
pub fn format_price(amount: u64) -> String {
    let digits = amount.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    format!("Rp {out}")
}

fn text(obj: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| obj.get(*k).and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn number(obj: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| obj.get(*k).and_then(extract_u64))
}

/// Objek donasi dari V1 (`data`) atau V2 (`item`). `None` jika bukan donasi.
pub fn donation_from(obj: &Value) -> Option<Donation> {
    obj.as_object()?;
    let name = text(obj, &["supporter_name", "name"]);
    let amount = number(obj, &["price", "amount", "total"]);
    let quantity = number(obj, &["quantity", "qty"]);
    // Tanpa nama, nominal, maupun jumlah: event lain, bukan donasi.
    if name.is_none() && amount.is_none() && quantity.is_none() {
        return None;
    }
    let amount_rp = amount.unwrap_or(0);
    Some(Donation {
        source_id: text(obj, &["id", "uuid"]).or_else(|| obj.get("id").and_then(Value::as_u64).map(|n| n.to_string())),
        supporter_name: name.unwrap_or_else(|| "Seseorang".into()),
        unit: text(obj, &["unit", "unit_name"]).unwrap_or_else(|| "Unit".into()),
        quantity: u32::try_from(quantity.unwrap_or(1).max(1)).unwrap_or(u32::MAX),
        amount_rp,
        price_text: format_price(amount_rp),
        message: text(obj, &["supporter_message", "support_message", "message"]).unwrap_or_default(),
        avatar_url: text(obj, &["supporter_avatar", "avatar"]).unwrap_or_else(|| DEFAULT_AVATAR.into()),
        unit_icon_url: text(obj, &["unit_icon", "unit_icon_url"]),
    })
}

/// `data` Pusher kadang berupa string JSON yang perlu diurai sekali lagi.
fn unwrap_data(v: &Value) -> Value {
    match v.get("data") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

/// Satu frame teks dari server V1 (Pusher).
pub fn parse_v1(frame: &str) -> Incoming {
    let Ok(v) = serde_json::from_str::<Value>(frame) else { return Incoming::Ignore };
    let event = v.get("event").and_then(Value::as_str).unwrap_or_default();
    let data = unwrap_data(&v);
    match event {
        "pusher:connection_established" => Incoming::Established {
            activity_timeout_s: data
                .get("activity_timeout")
                .and_then(extract_u64)
                .filter(|t| *t > 0)
                .unwrap_or(DEFAULT_ACTIVITY_TIMEOUT_S),
        },
        "pusher:ping" => Incoming::Ping,
        "pusher:error" => Incoming::Error(
            text(&data, &["message"]).unwrap_or_else(|| "Server Trakteer mengirim error.".into()),
        ),
        e if e.is_empty() || e.starts_with("pusher") => Incoming::Ignore,
        _ => {
            // Isi donasi kadang dibungkus `data` sekali lagi.
            let inner = match data.get("data") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(obj @ Value::Object(_)) => obj.clone(),
                _ => data,
            };
            let test = v.get("channel").and_then(Value::as_str).is_some_and(|c| c.contains("-test."));
            match donation_from(&inner) {
                Some(donation) => Incoming::Donation { donation, test },
                None => Incoming::Ignore,
            }
        }
    }
}

/// Satu frame teks dari server V2.
pub fn parse_v2(frame: &str) -> Incoming {
    let Ok(v) = serde_json::from_str::<Value>(frame) else { return Incoming::Ignore };
    match v.get("status").and_then(Value::as_str) {
        Some("authenticated") => return Incoming::Authenticated,
        Some("error" | "unauthorized" | "unauthenticated") => {
            return Incoming::Error(
                text(&v, &["message", "error"]).unwrap_or_else(|| "Stream Key ditolak Trakteer.".into()),
            )
        }
        _ => {}
    }
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default();
    let Some(event) = kind.strip_prefix("alert-queue:") else { return Incoming::Ignore };
    let Some(item) = v.get("item") else { return Incoming::Ignore };
    // Hanya saat alert benar-benar diputar, agar sinkron dengan widget alert Trakteer.
    let playing = item.get("status").and_then(Value::as_str) == Some("playing")
        || event == "play"
        || event == "test-alert";
    if !playing {
        return Incoming::Ignore;
    }
    let test = event.contains("test") || item.get("is_test").and_then(Value::as_bool).unwrap_or(false);
    match donation_from(item) {
        Some(donation) => Incoming::Donation { donation, test },
        None => Incoming::Ignore,
    }
}

pub fn parse(protocol: Protocol, frame: &str) -> Incoming {
    match protocol {
        Protocol::V1 => parse_v1(frame),
        Protocol::V2 => parse_v2(frame),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn donation(i: &Incoming) -> (&Donation, bool) {
        match i {
            Incoming::Donation { donation, test } => (donation, *test),
            other => panic!("bukan donasi: {other:?}"),
        }
    }

    #[test]
    fn keys() {
        // "dGVzdDEyMw==" = "test123"
        assert_eq!(parse_key(" dGVzdDEyMw== "), Ok(Credential::V1Channel("test123".into())));
        assert_eq!(parse_key("trv2-abc123"), Ok(Credential::V2Token("trv2-abc123".into())));
        for bad in ["", "   ", "trv2-", "bukan base64!", "dGVz dDEy", "AA=="] {
            let err = parse_key(bad).expect_err(bad);
            assert!(err.contains("trv2-"), "{bad}: {err}");
        }
    }

    #[test]
    fn urls() {
        let v1 = Credential::V1Channel("abc".into());
        assert!(v1.url_with_base(None).starts_with("wss://socket.trakteer.id/app/2ae25d102cc6cd41100a?protocol=7"));
        let v2 = Credential::V2Token("trv2-a+b".into());
        assert_eq!(v2.url_with_base(None), "wss://ws.trakteer.id/ws?token=trv2-a%2Bb&widget=alert-box");
        assert_eq!(v2.url_with_base(Some("ws://127.0.0.1:9")), "ws://127.0.0.1:9/ws?token=trv2-a%2Bb&widget=alert-box");
    }

    #[test]
    fn prices() {
        assert_eq!(format_price(0), "Rp 0");
        assert_eq!(format_price(5000), "Rp 5.000");
        assert_eq!(format_price(1_500_000), "Rp 1.500.000");
        assert_eq!(extract_u64(&json!("Rp 25.000")), Some(25_000));
        assert_eq!(extract_u64(&json!(3)), Some(3));
        assert_eq!(extract_u64(&json!(2.0)), Some(2));
        assert_eq!(extract_u64(&json!(null)), None);
    }

    #[test]
    fn v1_handshake_and_heartbeat() {
        let est = r#"{"event":"pusher:connection_established","data":"{\"socket_id\":\"123.456\",\"activity_timeout\":120}"}"#;
        assert_eq!(parse_v1(est), Incoming::Established { activity_timeout_s: 120 });
        let est_obj = r#"{"event":"pusher:connection_established","data":{"socket_id":"1.2"}}"#;
        assert_eq!(parse_v1(est_obj), Incoming::Established { activity_timeout_s: DEFAULT_ACTIVITY_TIMEOUT_S });
        assert_eq!(parse_v1(r#"{"event":"pusher:ping","data":{}}"#), Incoming::Ping);
        assert_eq!(
            parse_v1(r#"{"event":"pusher:error","data":{"code":4001,"message":"App key tidak dikenal"}}"#),
            Incoming::Error("App key tidak dikenal".into())
        );
        assert_eq!(
            parse_v1(r#"{"event":"pusher_internal:subscription_succeeded","channel":"creator-stream.x","data":"{}"}"#),
            Incoming::Ignore
        );
        assert_eq!(parse_v1("bukan json"), Incoming::Ignore);
    }

    #[test]
    fn v1_donation_double_encoded_and_nested() {
        // Data rekaan dengan bentuk seperti payload Trakteer V1.
        let inner = json!({ "data": {
            "supporter_name": "Penonton Uji", "unit": "Cendol", "quantity": 2,
            "supporter_message": "Semangat !jump", "supporter_avatar": "https://contoh.id/a.png",
            "unit_icon": "https://contoh.id/cendol.png", "price": "Rp 10.000",
            "media": { "gif": null }
        } });
        let frame = json!({ "event": "TipReceived", "channel": "creator-stream.abc", "data": inner.to_string() }).to_string();
        let parsed = parse_v1(&frame);
        let (d, test) = donation(&parsed);
        assert!(!test);
        assert_eq!(d.supporter_name, "Penonton Uji");
        assert_eq!(d.unit, "Cendol");
        assert_eq!(d.quantity, 2);
        assert_eq!(d.amount_rp, 10_000);
        assert_eq!(d.price_text, "Rp 10.000");
        assert_eq!(d.message, "Semangat !jump");
        assert_eq!(d.unit_icon_url.as_deref(), Some("https://contoh.id/cendol.png"));

        // Data sebagai objek, tanpa pembungkus `data`, di channel test.
        let frame = json!({ "event": "TipReceived", "channel": "creator-stream-test.abc",
            "data": { "supporter_name": "Uji", "unit": "Kopi", "quantity": "3", "price": "Rp 15.000" } }).to_string();
        let parsed = parse_v1(&frame);
        let (d, test) = donation(&parsed);
        assert!(test);
        assert_eq!((d.quantity, d.amount_rp), (3, 15_000));
        assert_eq!(d.message, "");
        assert!(d.avatar_url.contains("default-avatar"));
    }

    #[test]
    fn v1_non_donation_events_are_ignored() {
        let frame = json!({ "event": "SomethingElse", "channel": "creator-stream.abc", "data": "{\"foo\":1}" }).to_string();
        assert_eq!(parse_v1(&frame), Incoming::Ignore);
    }

    fn v2(kind: &str, item: Value) -> String {
        json!({ "type": kind, "item": item }).to_string()
    }

    #[test]
    fn v2_auth_and_errors() {
        assert_eq!(parse_v2(r#"{"status":"authenticated"}"#), Incoming::Authenticated);
        assert_eq!(
            parse_v2(r#"{"status":"error","message":"token tidak valid"}"#),
            Incoming::Error("token tidak valid".into())
        );
        assert_eq!(parse_v2(r#"{"type":"widget:config","item":{}}"#), Incoming::Ignore);
    }

    #[test]
    fn v2_only_playing_alerts_count() {
        let item = json!({ "id": "alert-1", "supporter_name": "Uji V2", "unit_name": "Bakso", "quantity": 1,
            "price": 20000, "support_message": "halo", "avatar": "https://contoh.id/b.png",
            "unit_icon_url": "https://contoh.id/bakso.png" });

        let queued = json!({ "status": "queued" });
        let mut not_playing = item.clone();
        not_playing.as_object_mut().unwrap().extend(queued.as_object().unwrap().clone());
        assert_eq!(parse_v2(&v2("alert-queue:added", not_playing)), Incoming::Ignore);

        let mut playing = item.clone();
        playing["status"] = json!("playing");
        let parsed = parse_v2(&v2("alert-queue:update", playing));
        let (d, test) = donation(&parsed);
        assert!(!test);
        assert_eq!(d.source_id.as_deref(), Some("alert-1"));
        assert_eq!(d.unit, "Bakso");
        assert_eq!(d.price_text, "Rp 20.000");
        assert_eq!(d.message, "halo");
        assert_eq!(d.avatar_url, "https://contoh.id/b.png");

        let parsed = parse_v2(&v2("alert-queue:play", item.clone()));
        assert!(!donation(&parsed).1);
        let parsed = parse_v2(&v2("alert-queue:test-alert", item));
        assert!(donation(&parsed).1, "test-alert ditandai test");
    }

    #[test]
    fn subscribe_both_channels() {
        let [live, test] = subscribe_messages("abc");
        assert!(live.contains("\"creator-stream.abc\"") && live.contains("pusher:subscribe"));
        assert!(test.contains("\"creator-stream-test.abc\""));
    }
}
