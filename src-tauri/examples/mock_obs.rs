//! Server OBS tiruan untuk mencoba aplikasi tanpa OBS asli.
//!
//! ```text
//! cargo run --example mock_obs -- [--port 4455] [--password RAHASIA] [--live] [--drop 2.5] [--kbps 6000] [--mic-muted]
//!     [--mic-db -20 | --mic-silent] [--mic-clip] [--stop-after 180]
//! ```
//!
//! - `--mic-silent`: mic tanpa suara sama sekali (uji alert "Mic tidak terdengar").
//! - `--mic-clip`: peak mic sesekali 0 dBFS (uji alert "Suara mic pecah").
//! - `--stop-after N`: stream dihentikan seperti dari OBS setelah N detik live (uji ringkasan sesi).
//!
//! Lalu arahkan dashboard → Koneksi ke `127.0.0.1` dan port yang sama.

#[path = "../tests/support/mock_obs.rs"]
mod mock_obs;

use mock_obs::{MockConfig, MockObs};

fn parse_args() -> Result<(u16, MockConfig), String> {
    let mut port = 4455;
    let mut config = MockConfig { verbose: true, ..MockConfig::default() };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} butuh nilai"));
        match arg.as_str() {
            "--port" => port = value("--port")?.parse().map_err(|e| format!("--port: {e}"))?,
            "--password" => config.password = Some(value("--password")?),
            "--live" => config.live = true,
            "--mic-muted" => config.mic_muted = true,
            "--drop" => config.drop_pct = value("--drop")?.parse().map_err(|e| format!("--drop: {e}"))?,
            "--kbps" => config.bitrate_kbps = value("--kbps")?.parse().map_err(|e| format!("--kbps: {e}"))?,
            "--mic-db" => {
                config.mic_level_db = Some(value("--mic-db")?.parse().map_err(|e| format!("--mic-db: {e}"))?)
            }
            "--mic-silent" => config.mic_level_db = None,
            "--mic-clip" => config.mic_clip = true,
            "--stop-after" => {
                config.stop_after_s = Some(value("--stop-after")?.parse().map_err(|e| format!("--stop-after: {e}"))?)
            }
            other => return Err(format!("argumen tidak dikenal: {other}")),
        }
    }
    Ok((port, config))
}

#[tokio::main]
async fn main() {
    let (port, config) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let auth = if config.password.is_some() { "dengan password" } else { "tanpa password" };
    let mock = match MockObs::start(port, config).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gagal membuka port {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("[mock-obs] mendengarkan di ws://127.0.0.1:{} ({auth}). Ctrl+C untuk berhenti.", mock.port);
    let _ = tokio::signal::ctrl_c().await;
}
