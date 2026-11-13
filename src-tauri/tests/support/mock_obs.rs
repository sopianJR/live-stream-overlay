//! Server obs-websocket v5 tiruan untuk pengujian tanpa menyentuh OBS asli.
//!
//! Dipakai oleh unit test modul `obs` (lewat `#[path]`) dan oleh
//! `cargo run --example mock_obs` untuk mencoba aplikasi secara end-to-end.
//! Hanya request yang dipakai aplikasi yang didukung; sisanya dijawab `UnknownRequestType`.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_websockets::{CloseCode, Message, ServerBuilder};

const CHALLENGE: &str = "bW9jay1jaGFsbGVuZ2U=";
const SALT: &str = "bW9jay1zYWx0";
const FPS: u64 = 60;

#[derive(Clone, Debug)]
pub struct MockConfig {
    pub password: Option<String>,
    /// Mulai dalam keadaan sedang live.
    pub live: bool,
    pub bitrate_kbps: u64,
    /// Persentase frame yang di-drop saat live (simulasi jaringan buruk).
    pub drop_pct: f64,
    /// Cetak setiap request ke stdout.
    pub verbose: bool,
    /// Input "Mic/Aux" mulai dalam keadaan mute.
    pub mic_muted: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self { password: None, live: false, bitrate_kbps: 6000, drop_pct: 0.0, verbose: false, mic_muted: false }
    }
}

struct Output {
    started: Option<Instant>,
    paused: bool,
}

impl Output {
    fn elapsed_ms(&self) -> u64 {
        self.started.map_or(0, |s| s.elapsed().as_millis() as u64)
    }
}

struct World {
    config: MockConfig,
    stream: Output,
    record: Output,
    scenes: Vec<String>,
    current_scene: String,
    /// Nama input → (jenis, mute).
    inputs: BTreeMap<String, (&'static str, bool)>,
    requests: Vec<String>,
}

fn uuid_for(name: &str) -> String {
    let n = name.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(u64::from(b)));
    format!("00000000-0000-4000-8000-{:012x}", n & 0xffff_ffff_ffff)
}

fn timecode(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}", s / 3600, s % 3600 / 60, s % 60, ms % 1000)
}

fn event(ty: &str, data: Value) -> Value {
    json!({ "op": 5, "d": { "eventType": ty, "eventIntent": 0, "eventData": data } })
}

fn output_event(ty: &str, active: bool, state: &str) -> Value {
    event(ty, json!({ "outputActive": active, "outputState": format!("OBS_WEBSOCKET_OUTPUT_{state}") }))
}

type Reply = Result<Value, (u16, String)>;

impl World {
    fn new(config: MockConfig) -> Self {
        let live = config.live;
        let mic_muted = config.mic_muted;
        let inputs = BTreeMap::from([
            ("Mic/Aux".to_string(), ("wasapi_input_capture", mic_muted)),
            ("Desktop Audio".to_string(), ("wasapi_output_capture", false)),
            ("Mic Podcast".to_string(), ("wasapi_input_capture", true)),
            ("Webcam".to_string(), ("dshow_input", false)),
        ]);
        Self {
            config,
            stream: Output { started: live.then(Instant::now), paused: false },
            record: Output { started: None, paused: false },
            // Urutan tampilan di OBS: paling atas dulu.
            scenes: vec!["Game".into(), "Just Chatting".into(), "BRB".into()],
            current_scene: "Game".into(),
            inputs,
            requests: Vec::new(),
        }
    }

    fn input_name(data: &Value) -> Option<&str> {
        data.get("inputName").and_then(Value::as_str)
    }

    fn handle(&mut self, ty: &str, data: &Value, events: &mut Vec<Value>) -> Reply {
        match ty {
            "GetVersion" => Ok(json!({
                "obsStudioVersion": "32.0.2",
                "obsWebSocketVersion": "5.6.3",
                "rpcVersion": 1,
                "availableRequests": [],
                "supportedImageFormats": ["png"],
                "platform": "windows",
                "platformDescription": "Mock OBS",
            })),
            "GetStreamStatus" => {
                let ms = self.stream.elapsed_ms();
                let total = ms * FPS / 1000;
                let skipped = (total as f64 * self.config.drop_pct / 100.0) as u64;
                Ok(json!({
                    "outputActive": self.stream.started.is_some(),
                    "outputReconnecting": false,
                    "outputTimecode": timecode(ms),
                    "outputDuration": ms,
                    "outputCongestion": if self.config.drop_pct > 0.0 { 0.4 } else { 0.0 },
                    "outputBytes": ms * self.config.bitrate_kbps / 8,
                    "outputSkippedFrames": skipped,
                    "outputTotalFrames": total,
                }))
            }
            "GetRecordStatus" => {
                let ms = self.record.elapsed_ms();
                Ok(json!({
                    "outputActive": self.record.started.is_some(),
                    "outputPaused": self.record.paused,
                    "outputTimecode": timecode(ms),
                    "outputDuration": ms,
                    "outputBytes": ms * 1000,
                }))
            }
            "GetStats" => {
                let frames = self.stream.elapsed_ms().max(1) * FPS / 1000;
                Ok(json!({
                    "cpuUsage": 4.2,
                    "memoryUsage": 420.0,
                    "availableDiskSpace": 120_000.0,
                    "activeFps": 60.0,
                    "averageFrameRenderTime": 1.4,
                    "renderSkippedFrames": 0,
                    "renderTotalFrames": frames,
                    "outputSkippedFrames": 0,
                    "outputTotalFrames": frames,
                    "webSocketSessionIncomingMessages": 0,
                    "webSocketSessionOutgoingMessages": 0,
                }))
            }
            "GetSceneList" => {
                let n = self.scenes.len();
                let scenes: Vec<Value> = self
                    .scenes
                    .iter()
                    .enumerate()
                    // OBS memberi indeks 0 untuk scene paling bawah.
                    .map(|(i, s)| json!({ "sceneIndex": n - 1 - i, "sceneName": s, "sceneUuid": uuid_for(s) }))
                    .rev()
                    .collect();
                Ok(json!({
                    "currentProgramSceneName": self.current_scene,
                    "currentProgramSceneUuid": uuid_for(&self.current_scene),
                    "scenes": scenes,
                }))
            }
            "SetCurrentProgramScene" => {
                let name = data.get("sceneName").and_then(Value::as_str).unwrap_or_default();
                if !self.scenes.iter().any(|s| s == name) {
                    return Err((600, format!("No source was found by the name of `{name}`.")));
                }
                self.current_scene = name.to_string();
                events.push(event(
                    "CurrentProgramSceneChanged",
                    json!({ "sceneName": name, "sceneUuid": uuid_for(name) }),
                ));
                Ok(Value::Null)
            }
            "GetSpecialInputs" => Ok(json!({
                "desktop1": "Desktop Audio", "desktop2": null,
                "mic1": "Mic/Aux", "mic2": null, "mic3": null, "mic4": null,
            })),
            "GetInputList" => Ok(json!({
                "inputs": self.inputs.iter().map(|(name, (kind, _))| json!({
                    "inputName": name, "inputUuid": uuid_for(name),
                    "inputKind": kind, "unversionedInputKind": kind,
                })).collect::<Vec<_>>(),
            })),
            "GetInputMute" | "ToggleInputMute" | "SetInputMute" => {
                let name = Self::input_name(data).unwrap_or_default().to_string();
                let Some((kind, muted)) = self.inputs.get_mut(&name) else {
                    return Err((600, format!("No source was found by the name of `{name}`.")));
                };
                if !kind.contains("capture") {
                    return Err((605, "The specified input does not support audio.".into()));
                }
                let before = *muted;
                match ty {
                    "ToggleInputMute" => *muted = !*muted,
                    "SetInputMute" => *muted = data.get("inputMuted").and_then(Value::as_bool).unwrap_or(*muted),
                    _ => {}
                }
                if *muted != before {
                    events.push(event(
                        "InputMuteStateChanged",
                        json!({ "inputName": name, "inputUuid": uuid_for(&name), "inputMuted": *muted }),
                    ));
                }
                Ok(json!({ "inputMuted": *muted }))
            }
            "StartStream" | "StopStream" | "ToggleStream" => {
                let active = self.stream.started.is_some();
                let start = match ty {
                    "StartStream" if active => return Err((500, "The stream output is already running.".into())),
                    "StopStream" if !active => return Err((501, "The stream output is not running.".into())),
                    "StartStream" => true,
                    "StopStream" => false,
                    _ => !active,
                };
                if start {
                    self.stream.started = Some(Instant::now());
                    events.push(output_event("StreamStateChanged", false, "STARTING"));
                    events.push(output_event("StreamStateChanged", true, "STARTED"));
                } else {
                    self.stream.started = None;
                    events.push(output_event("StreamStateChanged", true, "STOPPING"));
                    events.push(output_event("StreamStateChanged", false, "STOPPED"));
                }
                Ok(if ty == "ToggleStream" { json!({ "outputActive": start }) } else { Value::Null })
            }
            "StartRecord" | "StopRecord" | "ToggleRecord" => {
                let active = self.record.started.is_some();
                let start = match ty {
                    "StartRecord" if active => return Err((500, "The record output is already running.".into())),
                    "StopRecord" if !active => return Err((501, "The record output is not running.".into())),
                    "StartRecord" => true,
                    "StopRecord" => false,
                    _ => !active,
                };
                let path = "C:\\Video\\mock-recording.mkv";
                if start {
                    self.record = Output { started: Some(Instant::now()), paused: false };
                    events.push(output_event("RecordStateChanged", false, "STARTING"));
                    events.push(output_event("RecordStateChanged", true, "STARTED"));
                } else {
                    self.record = Output { started: None, paused: false };
                    events.push(output_event("RecordStateChanged", true, "STOPPING"));
                    events.push(event(
                        "RecordStateChanged",
                        json!({ "outputActive": false, "outputState": "OBS_WEBSOCKET_OUTPUT_STOPPED", "outputPath": path }),
                    ));
                }
                Ok(match ty {
                    "ToggleRecord" => json!({ "outputActive": start }),
                    "StopRecord" => json!({ "outputPath": path }),
                    _ => Value::Null,
                })
            }
            _ => Err((204, format!("Mock tidak mendukung request `{ty}`."))),
        }
    }
}

struct Shared {
    world: Mutex<World>,
    events: broadcast::Sender<Value>,
}

impl Shared {
    fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Server tiruan yang berjalan di latar. Semua koneksi ditutup saat nilai ini di-drop.
pub struct MockObs {
    pub port: u16,
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

impl MockObs {
    /// `port = 0` memilih port acak yang bebas.
    pub async fn start(port: u16, config: MockConfig) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        let port = listener.local_addr()?.port();
        let (events, _) = broadcast::channel(64);
        let shared = Arc::new(Shared { world: Mutex::new(World::new(config)), events });
        let accept_shared = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            // JoinSet membatalkan semua koneksi saat task ini dihentikan.
            let mut conns = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                let shared = Arc::clone(&accept_shared);
                conns.spawn(async move {
                    if let Err(e) = serve(stream, shared).await {
                        eprintln!("[mock-obs] koneksi berakhir: {e}");
                    }
                });
            }
        });
        Ok(Self { port, shared, task })
    }

    /// Daftar `requestType` yang sudah diterima, berurutan.
    pub fn requests(&self) -> Vec<String> {
        self.shared.world().requests.clone()
    }

    pub fn is_live(&self) -> bool {
        self.shared.world().stream.started.is_some()
    }

    pub fn is_recording(&self) -> bool {
        self.shared.world().record.started.is_some()
    }

    pub fn is_muted(&self, input: &str) -> Option<bool> {
        self.shared.world().inputs.get(input).map(|(_, m)| *m)
    }

    pub fn current_scene(&self) -> String {
        self.shared.world().current_scene.clone()
    }

    /// Mengubah persentase drop secara langsung (simulasi jaringan memburuk).
    pub fn set_drop_pct(&self, pct: f64) {
        self.shared.world().config.drop_pct = pct;
    }

    /// Menutup server dan semua koneksi (seperti OBS ditutup).
    pub fn shutdown(self) {
        self.task.abort();
    }
}

impl Drop for MockObs {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn auth_string(password: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    let secret = b64.encode(Sha256::digest(format!("{password}{SALT}")));
    b64.encode(Sha256::digest(format!("{secret}{CHALLENGE}")))
}

async fn serve(stream: TcpStream, shared: Arc<Shared>) -> Result<(), tokio_websockets::Error> {
    let (_req, mut ws) = ServerBuilder::new().accept(stream).await?;
    let (password, verbose) = {
        let w = shared.world();
        (w.config.password.clone(), w.config.verbose)
    };

    let mut hello = json!({ "obsWebSocketVersion": "5.6.3", "rpcVersion": 1 });
    if password.is_some() {
        hello["authentication"] = json!({ "challenge": CHALLENGE, "salt": SALT });
    }
    ws.send(Message::text(json!({ "op": 0, "d": hello }).to_string())).await?;

    let identify: Value = match ws.next().await {
        Some(Ok(msg)) => msg.as_text().and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default(),
        _ => return Ok(()),
    };
    if let Some(pw) = &password {
        let given = identify["d"]["authentication"].as_str();
        if given != Some(auth_string(pw).as_str()) {
            if verbose {
                println!("[mock-obs] autentikasi gagal");
            }
            let code = CloseCode::try_from(4009).expect("kode close valid");
            ws.send(Message::close(Some(code), "Authentication failed.")).await?;
            // Tunggu sebentar agar frame close sempat terkirim.
            let _ = tokio::time::timeout(Duration::from_millis(200), ws.next()).await;
            return Ok(());
        }
    }
    ws.send(Message::text(json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } }).to_string()))
        .await?;
    if verbose {
        println!("[mock-obs] klien terhubung");
    }

    let mut events = shared.events.subscribe();
    loop {
        tokio::select! {
            msg = ws.next() => {
                let Some(Ok(msg)) = msg else { break };
                if msg.is_close() {
                    break;
                }
                let Some(req) = msg.as_text().and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
                    continue;
                };
                let d = &req["d"];
                let ty = d["requestType"].as_str().unwrap_or_default().to_string();
                let id = d["requestId"].clone();
                let mut new_events = Vec::new();
                let reply = {
                    let mut w = shared.world();
                    w.requests.push(ty.clone());
                    w.handle(&ty, &d["requestData"], &mut new_events)
                };
                let is_poll = matches!(ty.as_str(), "GetStreamStatus" | "GetRecordStatus" | "GetStats");
                if verbose && !is_poll {
                    println!("[mock-obs] {ty} → {}", if reply.is_ok() { "ok" } else { "gagal" });
                }
                let status = match &reply {
                    Ok(_) => json!({ "result": true, "code": 100 }),
                    Err((code, comment)) => json!({ "result": false, "code": code, "comment": comment }),
                };
                let mut body = json!({ "requestType": ty, "requestId": id, "requestStatus": status });
                if let Ok(data) = &reply {
                    if !data.is_null() {
                        body["responseData"] = data.clone();
                    }
                }
                ws.send(Message::text(json!({ "op": 7, "d": body }).to_string())).await?;
                for ev in new_events {
                    let _ = shared.events.send(ev);
                }
            }
            ev = events.recv() => match ev {
                Ok(ev) => ws.send(Message::text(ev.to_string())).await?,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    Ok(())
}
