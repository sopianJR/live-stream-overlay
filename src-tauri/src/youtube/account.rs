//! Akun Google untuk membalas chat (opsional).
//!
//! - OAuth client (client ID + secret) milik user disimpan di Credential Manager.
//! - Setelah login, refresh token + nama channel disimpan di Credential Manager; token akses
//!   hanya di memori dan diperbarui otomatis.
//! - Tidak ada rahasia yang dikirim ke frontend.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use super::api::{self, ApiError};
use super::oauth::{self, ClientCredentials, Endpoints, LoginError, TokenFailure};
use super::{ChatService, ChatState};
use crate::secrets;

pub const STATUS_EVENT: &str = "account://status";
/// Token akses diperbarui jika sisa umurnya kurang dari ini.
const REFRESH_MARGIN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountState {
    SignedOut,
    SigningIn,
    SignedIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    /// OAuth client sudah diisi.
    pub has_client: bool,
    pub state: AccountState,
    pub channel_title: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredLogin {
    refresh_token: String,
    channel_id: String,
    channel_title: String,
}

struct Inner {
    status: AccountStatus,
    access: Option<(String, Instant)>,
    /// (video ID, live chat ID): menghemat 1 unit kuota per pesan.
    chat: Option<(String, String)>,
    cancel: Option<oneshot::Sender<()>>,
}

pub struct AccountService {
    http: Client,
    endpoints: Endpoints,
    inner: Mutex<Inner>,
    /// Satu pengiriman pada satu waktu (mencegah klik ganda).
    sending: tokio::sync::Mutex<()>,
}

impl AccountService {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> AccountStatus {
        self.lock().status.clone()
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(target: &str) -> Option<T> {
    match secrets::read(target) {
        Ok(Some(raw)) => serde_json::from_str(&raw).ok(),
        Ok(None) => None,
        Err(e) => {
            log::warn!("gagal membaca {target}: {e}");
            None
        }
    }
}

fn client_credentials() -> Option<ClientCredentials> {
    read_json(secrets::YOUTUBE_CLIENT_TARGET)
}

fn stored_login() -> Option<StoredLogin> {
    read_json(secrets::YOUTUBE_TOKEN_TARGET)
}

pub fn init(app: &AppHandle) {
    let login = stored_login();
    let status = AccountStatus {
        has_client: client_credentials().is_some(),
        state: if login.is_some() { AccountState::SignedIn } else { AccountState::SignedOut },
        channel_title: login.map(|l| l.channel_title),
        message: None,
    };
    let http = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| Client::new());
    app.manage(AccountService {
        http,
        endpoints: Endpoints::default(),
        inner: Mutex::new(Inner { status, access: None, chat: None, cancel: None }),
        sending: tokio::sync::Mutex::new(()),
    });
}

fn set_status(app: &AppHandle, f: impl FnOnce(&mut AccountStatus)) -> AccountStatus {
    let svc = app.state::<AccountService>();
    let status = {
        let mut inner = svc.lock();
        f(&mut inner.status);
        inner.status.clone()
    };
    if let Err(e) = app.emit(STATUS_EVENT, &status) {
        log::warn!("gagal emit status akun: {e}");
    }
    status
}

fn signed_out(app: &AppHandle, message: Option<String>) -> AccountStatus {
    let svc = app.state::<AccountService>();
    {
        let mut inner = svc.lock();
        inner.access = None;
        inner.chat = None;
    }
    set_status(app, |s| {
        s.state = AccountState::SignedOut;
        s.channel_title = None;
        s.message = message;
    })
}

// ---------- OAuth client ----------

pub fn set_client(app: &AppHandle, client_id: &str, client_secret: &str) -> Result<AccountStatus, String> {
    let creds = ClientCredentials::validated(client_id, client_secret)?;
    let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
    secrets::write(secrets::YOUTUBE_CLIENT_TARGET, &json)?;
    log::info!("OAuth client Google disimpan");
    Ok(set_status(app, |s| {
        s.has_client = true;
        s.message = None;
    }))
}

/// Menghapus OAuth client sekaligus login yang memakainya.
pub async fn clear_client(app: &AppHandle) -> Result<AccountStatus, String> {
    logout(app).await?;
    secrets::delete(secrets::YOUTUBE_CLIENT_TARGET)?;
    Ok(set_status(app, |s| s.has_client = false))
}

// ---------- Login ----------

pub async fn login(app: &AppHandle) -> Result<AccountStatus, String> {
    let client = client_credentials().ok_or("Isi Client ID dan client secret Google dulu.")?;
    let svc = app.state::<AccountService>();
    let (tx, rx) = oneshot::channel();
    {
        let mut inner = svc.lock();
        if inner.status.state == AccountState::SigningIn {
            return Err("Login sedang berjalan. Selesaikan di browser atau batalkan.".into());
        }
        inner.cancel = Some(tx);
    }
    set_status(app, |s| {
        s.state = AccountState::SigningIn;
        s.message = Some("Selesaikan login di browser yang terbuka…".into());
    });
    log::info!("login Google dimulai");

    let opener = app.clone();
    let open = move |url: &str| opener.opener().open_url(url, None::<&str>).map_err(|e| e.to_string());
    let result = oauth::authorize(&svc.http, &svc.endpoints, &client, open, rx, oauth::LOGIN_TIMEOUT).await;
    svc.lock().cancel = None;

    let tokens = match result {
        Ok(tokens) => tokens,
        Err(e) => {
            log::info!("login Google gagal: {e}");
            let message = (e != LoginError::Cancelled).then(|| e.to_string());
            let status = signed_out(app, message);
            return if e == LoginError::Cancelled { Ok(status) } else { Err(e.to_string()) };
        }
    };
    let Some(refresh_token) = tokens.refresh_token.clone() else {
        let msg = "Google tidak memberi izin akses jangka panjang. Coba login ulang.".to_string();
        signed_out(app, Some(msg.clone()));
        return Err(msg);
    };
    let channel = match api::my_channel(&svc.http, &svc.endpoints, &tokens.access_token).await {
        Ok(c) => c,
        Err(e) => {
            oauth::revoke(&svc.http, &svc.endpoints, &refresh_token).await;
            signed_out(app, Some(e.to_string()));
            return Err(e.to_string());
        }
    };

    let stored = StoredLogin { refresh_token, channel_id: channel.id, channel_title: channel.title.clone() };
    let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
    secrets::write(secrets::YOUTUBE_TOKEN_TARGET, &json)?;
    {
        let mut inner = svc.lock();
        let expires = Instant::now() + Duration::from_secs(tokens.expires_in.unwrap_or(3600));
        inner.access = Some((tokens.access_token, expires));
        inner.chat = None;
    }
    log::info!("login Google berhasil sebagai {}", channel.title);
    Ok(set_status(app, |s| {
        s.state = AccountState::SignedIn;
        s.channel_title = Some(channel.title);
        s.message = None;
    }))
}

pub fn cancel_login(app: &AppHandle) {
    if let Some(tx) = app.state::<AccountService>().lock().cancel.take() {
        let _ = tx.send(());
    }
}

pub async fn logout(app: &AppHandle) -> Result<AccountStatus, String> {
    cancel_login(app);
    if let Some(login) = stored_login() {
        let svc = app.state::<AccountService>();
        oauth::revoke(&svc.http, &svc.endpoints, &login.refresh_token).await;
    }
    secrets::delete(secrets::YOUTUBE_TOKEN_TARGET)?;
    log::info!("logout Google");
    Ok(signed_out(app, None))
}

// ---------- Kirim chat ----------

/// Token akses yang masih berlaku; diperbarui dengan refresh token jika perlu.
async fn access_token(app: &AppHandle, force_refresh: bool) -> Result<String, String> {
    let svc = app.state::<AccountService>();
    if !force_refresh {
        if let Some((token, expires)) = svc.lock().access.clone() {
            if expires > Instant::now() + REFRESH_MARGIN {
                return Ok(token);
            }
        }
    }
    let login = stored_login().ok_or("Belum login Google. Login di dashboard → Koneksi untuk membalas chat.")?;
    let client = client_credentials().ok_or("OAuth client Google belum diisi.")?;
    match oauth::refresh(&svc.http, &svc.endpoints, &client, &login.refresh_token).await {
        Ok(tokens) => {
            let expires = Instant::now() + Duration::from_secs(tokens.expires_in.unwrap_or(3600));
            svc.lock().access = Some((tokens.access_token.clone(), expires));
            Ok(tokens.access_token)
        }
        Err(TokenFailure::Revoked) => {
            let message = TokenFailure::Revoked.to_string();
            let _ = secrets::delete(secrets::YOUTUBE_TOKEN_TARGET);
            log::info!("refresh token Google tidak berlaku lagi");
            signed_out(app, Some(message.clone()));
            Err(message)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Video live yang chat-nya sedang ditampilkan overlay.
fn current_video(app: &AppHandle) -> Result<String, String> {
    let status = app.state::<ChatService>().status();
    match (status.state, status.video_id) {
        (ChatState::Connected, Some(id)) => Ok(id),
        _ => Err("Chat belum tersambung ke live, jadi belum ada tujuan pesan.".into()),
    }
}

async fn chat_id(app: &AppHandle, token: &str, video_id: &str) -> Result<String, ApiError> {
    let svc = app.state::<AccountService>();
    if let Some((video, chat)) = svc.lock().chat.clone() {
        if video == video_id {
            return Ok(chat);
        }
    }
    let chat = api::live_chat_id(&svc.http, &svc.endpoints, token, video_id).await?;
    svc.lock().chat = Some((video_id.to_string(), chat.clone()));
    Ok(chat)
}

async fn try_send(app: &AppHandle, token: &str, video_id: &str, text: &str) -> Result<(), ApiError> {
    let svc = app.state::<AccountService>();
    let chat = chat_id(app, token, video_id).await?;
    let result = api::send_message(&svc.http, &svc.endpoints, token, &chat, text).await;
    if matches!(result, Err(ApiError::ChatEnded | ApiError::NoActiveChat)) {
        svc.lock().chat = None;
    }
    result
}

pub async fn send_chat(app: &AppHandle, text: &str) -> Result<(), String> {
    let text = api::validate_message(text)?;
    let video_id = current_video(app)?;
    let svc = app.state::<AccountService>();
    let _guard = svc.sending.lock().await;

    let token = access_token(app, false).await?;
    let result = match try_send(app, &token, &video_id, &text).await {
        // Token akses ditolak: perbarui sekali lalu coba lagi.
        Err(ApiError::Unauthorized) => {
            let token = access_token(app, true).await?;
            try_send(app, &token, &video_id, &text).await
        }
        other => other,
    };
    match result {
        Ok(()) => {
            log::info!("pesan chat terkirim ({} karakter)", text.chars().count());
            Ok(())
        }
        Err(ApiError::Unauthorized) => {
            let message = ApiError::Unauthorized.to_string();
            signed_out(app, Some(message.clone()));
            Err(message)
        }
        Err(e) => {
            log::info!("pesan chat gagal: {e}");
            Err(e.to_string())
        }
    }
}
