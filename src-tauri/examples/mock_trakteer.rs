//! Server realtime Trakteer tiruan untuk mencoba alert donasi tanpa donasi asli.
//!
//! ```text
//! cargo run --example mock_trakteer -- [--port 4470] [--every 8]
//! ```
//!
//! Lalu jalankan aplikasi (build debug) dengan `LSO_TRAKTEER_URL=ws://127.0.0.1:4470` dan isi
//! kunci uji apa pun di dashboard, mis. `trv2-uji` (V2) atau `a2FuYWwtdWpp` (V1, "kanal-uji").

#[path = "../tests/support/mock_trakteer.rs"]
mod mock_trakteer;

use std::time::Duration;

use mock_trakteer::{MockDonation, MockTrakteer};

const SAMPLES: [(&str, u64, &str); 4] = [
    ("Penonton Satu", 5_000, "Halo bang, semangat!"),
    ("Sultan Rekaan", 100_000, "Gas ranked malam ini 🔥"),
    ("Penonton Dua", 10_000, ""),
    ("Teman Lama", 25_000, "Salam dari kampung halaman"),
];

#[tokio::main]
async fn main() {
    let mut port = 4470u16;
    let mut every = 8u64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().and_then(|v| v.parse().ok()).unwrap_or(port),
            "--every" => every = args.next().and_then(|v| v.parse().ok()).unwrap_or(every).max(1),
            other => {
                eprintln!("argumen tidak dikenal: {other}");
                std::process::exit(2);
            }
        }
    }
    let mock = match MockTrakteer::start(port).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gagal membuka port {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("[mock-trakteer] {} — donasi rekaan tiap {every} detik. Ctrl+C untuk berhenti.", mock.base_url());

    let mut tick = tokio::time::interval(Duration::from_secs(every));
    tick.tick().await;
    let mut i = 0usize;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let (name, amount, message) = SAMPLES[i % SAMPLES.len()];
                let mut donation = MockDonation::new(name, amount, i % 4 == 3);
                donation.message = message.into();
                donation.quantity = u32::try_from(amount / 5_000).unwrap_or(1).max(1);
                println!(
                    "[mock-trakteer] kirim donasi {} Rp {amount}{} ({} koneksi sejak mulai)",
                    i + 1,
                    if donation.test { " (test)" } else { "" },
                    mock.connections()
                );
                mock.send_donation(donation);
                i += 1;
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
}
