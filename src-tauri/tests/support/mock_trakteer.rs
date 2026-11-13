//! Server realtime Trakteer tiruan (V1 Pusher di `/app/...`, V2 di `/ws`) untuk test dan
//! `cargo run --example mock_trakteer`. Semua data donasi di sini rekaan.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_websockets::{Message, ServerBuilder};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct MockDonation {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub quantity: u32,
    pub amount: u64,
    pub message: String,
    pub test: bool,
}

impl MockDonation {
    pub fn new(name: &str, amount: u64, test: bool) -> Self {
        Self {
            id: format!("mock-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            name: name.into(),
            unit: "Cendol".into(),
            quantity: 1,
            amount,
            message: "pesan uji".into(),
            test,
        }
    }
}

#[derive(Clone, Debug)]
enum Push {
    Donation(MockDonation),
    Ping,
    Close,
}

#[derive(Default)]
struct State {
    subscriptions: Vec<String>,
    pongs: usize,
    connections: usize,
}

pub struct MockTrakteer {
    pub port: u16,
    state: Arc<Mutex<State>>,
    tx: broadcast::Sender<Push>,
    task: tokio::task::JoinHandle<()>,
}

impl MockTrakteer {
    /// `port = 0` memilih port acak.
    pub async fn start(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(Mutex::new(State::default()));
        let (tx, _) = broadcast::channel(64);
        let accept_state = Arc::clone(&state);
        let accept_tx = tx.clone();
        let task = tokio::spawn(async move {
            let mut conns = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                let state = Arc::clone(&accept_state);
                let rx = accept_tx.subscribe();
                conns.spawn(async move {
                    let _ = serve(stream, state, rx).await;
                });
            }
        });
        Ok(Self { port, state, tx, task })
    }

    pub fn base_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    pub fn send_donation(&self, donation: MockDonation) {
        let _ = self.tx.send(Push::Donation(donation));
    }

    /// V1: server mengirim `pusher:ping`, klien harus membalas pong.
    pub fn send_ping(&self) {
        let _ = self.tx.send(Push::Ping);
    }

    /// Memutus semua koneksi (seperti server restart).
    pub fn disconnect_all(&self) {
        let _ = self.tx.send(Push::Close);
    }

    pub fn subscriptions(&self) -> Vec<String> {
        self.state.lock().unwrap().subscriptions.clone()
    }

    pub fn pongs_received(&self) -> usize {
        self.state.lock().unwrap().pongs
    }

    pub fn connections(&self) -> usize {
        self.state.lock().unwrap().connections
    }
}

impl Drop for MockTrakteer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn item_json(d: &MockDonation) -> Value {
    json!({
        "id": d.id,
        "supporter_name": d.name,
        "unit_name": d.unit,
        "quantity": d.quantity,
        "price": d.amount,
        "support_message": d.message,
        "avatar": "https://edge-cdn.trakteer.id/images/mix/default-avatar.png",
        "status": "playing",
    })
}

fn v1_payload(d: &MockDonation) -> Value {
    // Bentuk V1: string JSON berisi `{ "data": { ... } }`, harga berupa teks.
    let inner = json!({ "data": {
        "supporter_name": d.name, "unit": d.unit, "quantity": d.quantity,
        "supporter_message": d.message, "price": format!("Rp {}", d.amount),
        "supporter_avatar": "https://edge-cdn.trakteer.id/images/mix/default-avatar.png",
        "unit_icon": "https://edge-cdn.trakteer.id/images/mix/cendol.png",
    } });
    Value::String(inner.to_string())
}

async fn serve(
    stream: TcpStream,
    state: Arc<Mutex<State>>,
    mut rx: broadcast::Receiver<Push>,
) -> Result<(), tokio_websockets::Error> {
    let (request, mut ws) = ServerBuilder::new().accept(stream).await?;
    let v2 = request.uri().path() == "/ws";
    let query = request.uri().query().unwrap_or_default().to_string();
    state.lock().unwrap().connections += 1;

    if v2 {
        let token = url::form_urlencoded::parse(query.as_bytes())
            .find(|(k, _)| k == "token")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
        if token == "trv2-salah" {
            ws.send(Message::text(json!({ "status": "error", "message": "token tidak valid" }).to_string())).await?;
            let _ = ws.send(Message::close(None, "")).await;
            return Ok(());
        }
        ws.send(Message::text(json!({ "status": "authenticated" }).to_string())).await?;
    } else {
        let data = json!({ "socket_id": "123.456", "activity_timeout": 120 }).to_string();
        ws.send(Message::text(json!({ "event": "pusher:connection_established", "data": data }).to_string()))
            .await?;
    }

    let mut channels: Vec<String> = Vec::new();
    loop {
        tokio::select! {
            msg = ws.next() => {
                let Some(Ok(msg)) = msg else { break };
                if msg.is_close() {
                    break;
                }
                let Some(v) = msg.as_text().and_then(|t| serde_json::from_str::<Value>(t).ok()) else { continue };
                match v["event"].as_str().unwrap_or_default() {
                    "pusher:subscribe" => {
                        let channel = v["data"]["channel"].as_str().unwrap_or_default().to_string();
                        channels.push(channel.clone());
                        state.lock().unwrap().subscriptions.push(channel.clone());
                        let ok = json!({ "event": "pusher_internal:subscription_succeeded", "channel": channel, "data": "{}" });
                        ws.send(Message::text(ok.to_string())).await?;
                    }
                    "pusher:ping" => {
                        ws.send(Message::text(r#"{"event":"pusher:pong","data":{}}"#)).await?;
                    }
                    "pusher:pong" => state.lock().unwrap().pongs += 1,
                    _ => {}
                }
            }
            push = rx.recv() => match push {
                Ok(Push::Donation(d)) => {
                    let frame = if v2 {
                        let kind = if d.test { "alert-queue:test-alert" } else { "alert-queue:play" };
                        json!({ "type": kind, "item": item_json(&d) })
                    } else {
                        let prefix = if d.test { "creator-stream-test." } else { "creator-stream." };
                        let channel = channels
                            .iter()
                            .find(|c| c.starts_with(prefix))
                            .cloned()
                            .unwrap_or_else(|| format!("{prefix}unknown"));
                        json!({ "event": "TipReceived", "channel": channel, "data": v1_payload(&d) })
                    };
                    ws.send(Message::text(frame.to_string())).await?;
                }
                Ok(Push::Ping) if !v2 => {
                    ws.send(Message::text(r#"{"event":"pusher:ping","data":{}}"#)).await?;
                }
                Ok(Push::Ping) => {}
                Ok(Push::Close) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    Ok(())
}
