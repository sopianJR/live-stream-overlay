//! Cek cepat apakah chat sebuah live YouTube bisa dibaca, tanpa membuka aplikasi.
//!
//! ```text
//! cargo run --example yt_chat -- @namachannel [detik]
//! cargo run --example yt_chat -- https://www.youtube.com/watch?v=XXXXXXXXXXX 60
//! ```

use std::time::{Duration, Instant};

use livestream_overlay_lib::youtube::innertube::Innertube;
use livestream_overlay_lib::youtube::parse::{ChatAction, ChatMessage, MessageKind, Run};
use livestream_overlay_lib::youtube::source::{self, Source};

fn line(m: &ChatMessage) -> String {
    let text: String = m
        .runs
        .iter()
        .map(|r| match r {
            Run::Text { text } => text.clone(),
            Run::Emoji { alt, .. } => alt.clone(),
        })
        .collect();
    let mut tags = Vec::new();
    if m.author.owner {
        tags.push("pemilik");
    }
    if m.author.moderator {
        tags.push("mod");
    }
    if m.author.member {
        tags.push("member");
    }
    let tags = if tags.is_empty() { String::new() } else { format!(" [{}]", tags.join(",")) };
    let extra = match m.kind {
        MessageKind::Text => String::new(),
        kind => format!(" <{kind:?} {} {}>", m.amount.as_deref().unwrap_or(""), m.header.as_deref().unwrap_or("")),
    };
    format!("{}{tags}:{extra} {text}", m.author.name)
}

fn print_actions(actions: Vec<ChatAction>) -> usize {
    let mut n = 0;
    for action in actions {
        match action {
            ChatAction::Upsert { message, .. } => {
                n += 1;
                println!("  {}", line(&message));
            }
            ChatAction::Remove(id) => println!("  (pesan {id} dihapus)"),
            ChatAction::RemoveByAuthor(c) => println!("  (semua pesan {c} dihapus)"),
        }
    }
    n
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("pakai: cargo run --example yt_chat -- <@handle | URL> [detik]");
        std::process::exit(2);
    };
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);

    let source = source::parse(&input).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let yt = Innertube::new().expect("klien HTTP");

    let video_id = match &source {
        Source::Video(id) => id.clone(),
        Source::Channel(path) => {
            let page = yt.live_page(path).await.unwrap_or_else(|e| {
                eprintln!("gagal membuka halaman channel: {e}");
                std::process::exit(1);
            });
            match page.video_id {
                Some(id) if page.live_now => id,
                Some(id) if page.upcoming => {
                    println!("live {id} dijadwalkan, belum dimulai");
                    return;
                }
                _ => {
                    println!("channel belum live");
                    return;
                }
            }
        }
    };

    let info = yt.video(&video_id).await.expect("info video");
    println!(
        "video {video_id}: {} — {} · live={} · penonton={:?}",
        info.title.as_deref().unwrap_or("?"),
        info.channel.as_deref().unwrap_or("?"),
        info.is_live,
        info.viewers
    );
    let Some(token) = info.chat_token else {
        println!("tidak ada live chat");
        return;
    };
    let first = yt.chat_page(&token).await.expect("chat").expect("halaman chat");
    let mut page = match &first.live_chat_token {
        Some(live) => {
            println!("beralih ke mode Live chat");
            yt.chat_page(live).await.expect("chat").unwrap_or(first)
        }
        None => first,
    };
    println!("riwayat:");
    let history = print_actions(std::mem::take(&mut page.actions));
    println!("({history} pesan riwayat) — mendengarkan {seconds} detik…");

    let start = Instant::now();
    let mut next = page.next;
    let mut total = 0;
    while start.elapsed() < Duration::from_secs(seconds) {
        let Some(cont) = next.take() else {
            println!("chat berakhir");
            break;
        };
        tokio::time::sleep(Duration::from_millis(1500)).await;
        match yt.chat_page(&cont.token).await {
            Ok(Some(p)) => {
                total += print_actions(p.actions);
                next = p.next;
            }
            Ok(None) => println!("chat berakhir"),
            Err(e) => {
                println!("gangguan: {e}");
                next = Some(cont);
            }
        }
    }
    let viewers = yt.viewership(&video_id).await.ok().flatten();
    println!("{total} pesan baru · penonton sekarang: {:?}", viewers.and_then(|v| v.viewers));
}
