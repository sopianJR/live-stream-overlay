//! Mengurai respons InnerTube (JSON / HTML) menjadi data chat. Fungsi murni, toleran:
//! field yang hilang atau berubah bentuk dilewati, tidak pernah panic.

use serde::Serialize;
use serde_json::Value;

// ---------- Tipe untuk frontend ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Run {
    Text { text: String },
    /// Emoji kustom channel (gambar). Emoji Unicode biasa menjadi `Text`.
    Emoji { alt: String, url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageKind {
    Text,
    SuperChat,
    SuperSticker,
    /// Member baru atau milestone membership.
    Membership,
    /// Seseorang menghadiahkan membership.
    GiftPurchase,
    /// Seseorang menerima membership hadiah.
    GiftRedemption,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub name: String,
    pub channel_id: String,
    pub photo: Option<String>,
    pub owner: bool,
    pub moderator: bool,
    pub verified: bool,
    pub member: bool,
    pub member_badge: Option<String>,
    /// Mis. "Member (6 bulan)".
    pub member_label: Option<String>,
}

/// Warna dari YouTube (sesuai tier Super Chat), sudah dalam format CSS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaidColors {
    pub header_bg: String,
    pub header_text: String,
    pub body_bg: String,
    pub body_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub kind: MessageKind,
    pub author: Author,
    pub runs: Vec<Run>,
    pub timestamp_ms: i64,
    /// Nominal Super Chat / Super Sticker, mis. "Rp 50.000".
    pub amount: Option<String>,
    /// Teks pembuka untuk membership/hadiah, atau label stiker.
    pub header: Option<String>,
    pub sticker: Option<String>,
    pub colors: Option<PaidColors>,
}

// Hampir semua aksi berupa Upsert dan hanya hidup sebentar per halaman; boxing tidak sepadan.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
    /// Pesan baru, atau pengganti pesan `replaces` (mis. placeholder yang sudah final).
    Upsert { replaces: Option<String>, message: ChatMessage },
    Remove(String),
    /// Semua pesan dari channel ini dihapus (user di-ban/timeout).
    RemoveByAuthor(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    pub token: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPage {
    pub actions: Vec<ChatAction>,
    /// `None` = chat sudah berakhir.
    pub next: Option<Continuation>,
    /// Token untuk mode "Live chat" (semua pesan) jika halaman ini masih mode "Top chat".
    pub live_chat_token: Option<String>,
}

// ---------- Helper JSON ----------

fn at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |cur, key| cur.get(key))
}

fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    at(v, path).and_then(Value::as_str).map(str::to_string)
}

fn https(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        url.to_string()
    }
}

/// URL thumbnail terbesar (YouTube mengurutkan dari kecil ke besar).
fn thumb(v: &Value) -> Option<String> {
    v.get("thumbnails")?.as_array()?.iter().rev().find_map(|t| t.get("url")?.as_str()).map(https)
}

fn emoji_run(emoji: &Value) -> Option<Run> {
    let id = emoji.get("emojiId").and_then(Value::as_str).unwrap_or_default();
    let custom = emoji.get("isCustomEmoji").and_then(Value::as_bool).unwrap_or(false);
    if !custom {
        // Emoji Unicode: ID-nya adalah karakter emoji itu sendiri.
        return (!id.is_empty()).then(|| Run::Text { text: id.to_string() });
    }
    let alt = emoji
        .get("shortcuts")
        .and_then(|s| s.get(0))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| str_at(emoji, &["image", "accessibility", "accessibilityData", "label"]))
        .unwrap_or_else(|| ":emoji:".to_string());
    match emoji.get("image").and_then(thumb) {
        Some(url) => Some(Run::Emoji { alt, url }),
        None => Some(Run::Text { text: alt }),
    }
}

fn push_text(runs: &mut Vec<Run>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Run::Text { text: last }) = runs.last_mut() {
        last.push_str(text);
    } else {
        runs.push(Run::Text { text: text.to_string() });
    }
}

/// Teks YouTube berbentuk `{simpleText}` atau `{runs: [...]}`.
fn runs_of(v: Option<&Value>) -> Vec<Run> {
    let Some(v) = v else { return Vec::new() };
    let mut runs = Vec::new();
    if let Some(text) = v.get("simpleText").and_then(Value::as_str) {
        push_text(&mut runs, text);
        return runs;
    }
    for run in v.get("runs").and_then(Value::as_array).into_iter().flatten() {
        if let Some(text) = run.get("text").and_then(Value::as_str) {
            push_text(&mut runs, text);
        } else if let Some(emoji) = run.get("emoji").and_then(emoji_run) {
            match emoji {
                Run::Text { text } => push_text(&mut runs, &text),
                image => runs.push(image),
            }
        }
    }
    runs
}

fn plain_text(v: Option<&Value>) -> Option<String> {
    let text: String = runs_of(v)
        .into_iter()
        .map(|r| match r {
            Run::Text { text } => text,
            Run::Emoji { alt, .. } => alt,
        })
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Warna ARGB (angka) → `rgba(r, g, b, a)`.
fn color(v: Option<&Value>) -> Option<String> {
    let argb = u32::try_from(v?.as_u64()?).ok()?;
    let [a, r, g, b] = argb.to_be_bytes();
    let alpha = (f64::from(a) / 255.0 * 1000.0).round() / 1000.0;
    Some(format!("rgba({r}, {g}, {b}, {alpha})"))
}

fn timestamp_ms(r: &Value) -> i64 {
    r.get("timestampUsec")
        .and_then(|t| t.as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| t.as_i64()))
        .map_or(0, |usec| usec / 1000)
}

fn author_of(r: &Value, channel_id: Option<&str>) -> Author {
    let mut author = Author {
        name: plain_text(r.get("authorName")).unwrap_or_default(),
        channel_id: channel_id
            .map(str::to_string)
            .or_else(|| str_at(r, &["authorExternalChannelId"]))
            .unwrap_or_default(),
        photo: r.get("authorPhoto").and_then(thumb),
        ..Author::default()
    };
    for badge in r.get("authorBadges").and_then(Value::as_array).into_iter().flatten() {
        let Some(b) = badge.get("liveChatAuthorBadgeRenderer") else { continue };
        match b.get("icon").and_then(|i| i.get("iconType")).and_then(Value::as_str) {
            Some("OWNER") => author.owner = true,
            Some("MODERATOR") => author.moderator = true,
            Some("VERIFIED") => author.verified = true,
            _ => {
                if let Some(url) = b.get("customThumbnail").and_then(thumb) {
                    author.member = true;
                    author.member_badge = Some(url);
                    author.member_label = str_at(b, &["tooltip"]);
                }
            }
        }
    }
    author
}

// ---------- Pesan chat ----------

fn message(id: String, kind: MessageKind, author: Author, runs: Vec<Run>, ts: i64) -> ChatMessage {
    ChatMessage { id, kind, author, runs, timestamp_ms: ts, amount: None, header: None, sticker: None, colors: None }
}

/// Satu `item` dari `addChatItemAction` / `replaceChatItemAction`. Jenis yang tidak
/// ditampilkan (placeholder, pesan sistem, dll.) menghasilkan `None`.
pub fn parse_item(item: &Value) -> Option<ChatMessage> {
    let (kind, r) = item.as_object()?.iter().next()?;
    let id = r.get("id")?.as_str()?.to_string();
    let ts = timestamp_ms(r);

    let msg = match kind.as_str() {
        "liveChatTextMessageRenderer" => {
            let runs = runs_of(r.get("message"));
            if runs.is_empty() {
                return None;
            }
            message(id, MessageKind::Text, author_of(r, None), runs, ts)
        }
        "liveChatPaidMessageRenderer" => ChatMessage {
            amount: plain_text(r.get("purchaseAmountText")),
            colors: Some(PaidColors {
                header_bg: color(r.get("headerBackgroundColor"))?,
                header_text: color(r.get("headerTextColor"))?,
                body_bg: color(r.get("bodyBackgroundColor"))?,
                body_text: color(r.get("bodyTextColor"))?,
            }),
            ..message(id, MessageKind::SuperChat, author_of(r, None), runs_of(r.get("message")), ts)
        },
        "liveChatPaidStickerRenderer" => ChatMessage {
            amount: plain_text(r.get("purchaseAmountText")),
            sticker: r.get("sticker").and_then(thumb),
            header: str_at(r, &["sticker", "accessibility", "accessibilityData", "label"]),
            colors: Some(PaidColors {
                header_bg: color(r.get("moneyChipBackgroundColor"))?,
                header_text: color(r.get("moneyChipTextColor"))?,
                body_bg: color(r.get("backgroundColor"))?,
                body_text: color(r.get("moneyChipTextColor"))?,
            }),
            ..message(id, MessageKind::SuperSticker, author_of(r, None), Vec::new(), ts)
        },
        "liveChatMembershipItemRenderer" => {
            // Milestone: headerPrimaryText "Member selama 6 bulan" + pesan; member baru: hanya headerSubText.
            let header = plain_text(r.get("headerPrimaryText")).or_else(|| plain_text(r.get("headerSubText")));
            ChatMessage {
                header,
                ..message(id, MessageKind::Membership, author_of(r, None), runs_of(r.get("message")), ts)
            }
        }
        "liveChatSponsorshipsGiftPurchaseAnnouncementRenderer" => {
            let h = at(r, &["header", "liveChatSponsorshipsHeaderRenderer"])?;
            let channel = r.get("authorExternalChannelId").and_then(Value::as_str);
            ChatMessage {
                header: plain_text(h.get("primaryText")),
                ..message(id, MessageKind::GiftPurchase, author_of(h, channel), Vec::new(), ts)
            }
        }
        "liveChatSponsorshipsGiftRedemptionAnnouncementRenderer" => {
            message(id, MessageKind::GiftRedemption, author_of(r, None), runs_of(r.get("message")), ts)
        }
        _ => return None,
    };
    Some(msg)
}

fn parse_action(action: &Value) -> Option<ChatAction> {
    let (kind, a) = action.as_object()?.iter().find(|(k, _)| k.ends_with("Action"))?;
    match kind.as_str() {
        "addChatItemAction" => {
            parse_item(a.get("item")?).map(|message| ChatAction::Upsert { replaces: None, message })
        }
        "replaceChatItemAction" => parse_item(a.get("replacementItem")?).map(|message| ChatAction::Upsert {
            replaces: str_at(a, &["targetItemId"]),
            message,
        }),
        "removeChatItemAction" | "markChatItemAsDeletedAction" => {
            str_at(a, &["targetItemId"]).map(ChatAction::Remove)
        }
        "removeChatItemByAuthorAction" | "markChatItemsByAuthorAsDeletedAction" => {
            str_at(a, &["externalChannelId"]).map(ChatAction::RemoveByAuthor)
        }
        _ => None,
    }
}

fn continuation_of(lcc: &Value) -> Option<Continuation> {
    let first = lcc.get("continuations")?.as_array()?.first()?.as_object()?;
    first.values().find_map(|data| {
        Some(Continuation {
            token: data.get("continuation")?.as_str()?.to_string(),
            timeout_ms: data.get("timeoutMs").and_then(Value::as_u64),
        })
    })
}

/// Pilihan "Top chat" / "Live chat" di header chat. Urutan YouTube: Top chat lalu Live chat.
fn live_chat_token(lcc: &Value) -> Option<String> {
    let items = at(
        lcc,
        &["header", "liveChatHeaderRenderer", "viewSelector", "sortFilterSubMenuRenderer", "subMenuItems"],
    )?
    .as_array()?;
    let live = items.get(1)?;
    if live.get("selected").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    str_at(live, &["continuation", "reloadContinuationData", "continuation"])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageError {
    /// Respons tidak berisi live chat (chat berakhir, dimatikan, atau token kedaluwarsa).
    NoChat,
}

/// Respons `live_chat/get_live_chat`.
pub fn parse_chat_page(v: &Value) -> Result<ChatPage, PageError> {
    let lcc = at(v, &["continuationContents", "liveChatContinuation"]).ok_or(PageError::NoChat)?;
    let actions = lcc
        .get("actions")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(parse_action).collect())
        .unwrap_or_default();
    Ok(ChatPage { actions, next: continuation_of(lcc), live_chat_token: live_chat_token(lcc) })
}

// ---------- Info video ----------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoInfo {
    pub title: Option<String>,
    pub channel: Option<String>,
    /// Token awal chat (mode Top chat). `None` = tidak ada live chat.
    pub chat_token: Option<String>,
    pub is_replay: bool,
    pub is_live: bool,
    pub viewers: Option<u64>,
}

fn view_count(renderer: &Value) -> Option<u64> {
    renderer
        .get("originalViewCount")
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_u64()))
}

/// Respons `next` (halaman tonton).
pub fn parse_next(v: &Value) -> VideoInfo {
    let mut info = VideoInfo::default();
    let Some(twc) = at(v, &["contents", "twoColumnWatchNextResults"]) else { return info };

    for c in at(twc, &["results", "results", "contents"]).and_then(Value::as_array).into_iter().flatten() {
        if let Some(p) = c.get("videoPrimaryInfoRenderer") {
            info.title = plain_text(p.get("title"));
            if let Some(vc) = at(p, &["viewCount", "videoViewCountRenderer"]) {
                info.is_live = vc.get("isLive").and_then(Value::as_bool).unwrap_or(false);
                info.viewers = if info.is_live { view_count(vc) } else { None };
            }
        }
        if let Some(owner) = at(c, &["videoSecondaryInfoRenderer", "owner", "videoOwnerRenderer"]) {
            info.channel = plain_text(owner.get("title"));
        }
    }

    if let Some(chat) = at(twc, &["conversationBar", "liveChatRenderer"]) {
        info.is_replay = chat.get("isReplay").and_then(Value::as_bool).unwrap_or(false);
        info.chat_token = chat
            .get("continuations")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| str_at(c, &["reloadContinuationData", "continuation"]));
    }
    info
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewership {
    pub viewers: Option<u64>,
    pub is_live: bool,
}

/// Respons `updated_metadata`. `None` jika tidak ada data penonton.
pub fn parse_viewership(v: &Value) -> Option<Viewership> {
    v.get("actions")?.as_array()?.iter().find_map(|a| {
        let vc = at(a, &["updateViewershipAction", "viewCount", "videoViewCountRenderer"])?;
        let is_live = vc.get("isLive").and_then(Value::as_bool).unwrap_or(false);
        Some(Viewership { viewers: if is_live { view_count(vc) } else { None }, is_live })
    })
}

// ---------- Halaman /live channel ----------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LivePage {
    /// Video yang dituju `/live` (sedang live atau dijadwalkan). `None` = channel tidak sedang live.
    pub video_id: Option<String>,
    pub live_now: bool,
    pub upcoming: bool,
    pub client_version: Option<String>,
}

fn between<'a>(html: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = html.find(start)? + start.len();
    let len = html[from..].find(end)?;
    Some(&html[from..from + len])
}

/// HTML `https://www.youtube.com/<channel>/live`.
pub fn parse_live_page(html: &str) -> LivePage {
    let canonical = between(html, "<link rel=\"canonical\" href=\"", "\"").unwrap_or_default();
    let video_id = canonical
        .split_once("watch?v=")
        .map(|(_, rest)| rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect::<String>())
        .filter(|id| id.len() == 11);
    LivePage {
        live_now: video_id.is_some() && html.contains("\"isLiveNow\":true"),
        upcoming: video_id.is_some() && html.contains("\"isUpcoming\":true"),
        video_id,
        client_version: between(html, "\"INNERTUBE_CLIENT_VERSION\":\"", "\"").map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!("{}/tests/fixtures/youtube/{name}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn upserts(page: &ChatPage) -> Vec<&ChatMessage> {
        page.actions
            .iter()
            .filter_map(|a| match a {
                ChatAction::Upsert { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn first_page_text_messages_and_live_chat_switch() {
        let page = parse_chat_page(&fixture("chat_first_page.json")).unwrap();
        assert_eq!(page.live_chat_token.as_deref(), Some("TOKEN_LIVE_CHAT"));
        assert_eq!(page.next, Some(Continuation { token: "TOKEN_NEXT_1".into(), timeout_ms: Some(10000) }));

        let msgs = upserts(&page);
        // Pesan sistem (viewer engagement) dan placeholder tidak ditampilkan.
        assert_eq!(msgs.len(), 3);

        let first = msgs[0];
        assert_eq!(first.kind, MessageKind::Text);
        assert_eq!(first.author.name, "@penonton_satu");
        assert_eq!(first.author.channel_id, "UCaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(first.timestamp_ms, 1_794_535_482_051);
        // Emoji Unicode digabung ke teks; emoji kustom menjadi gambar.
        assert_eq!(
            first.runs,
            vec![
                Run::Text { text: "halo bang 👋 ".into() },
                Run::Emoji { alt: ":yt:".into(), url: "https://yt3.ggpht.com/emoji-besar".into() },
            ]
        );
        assert_eq!(first.author.photo.as_deref(), Some("https://yt3.ggpht.com/foto-64"));

        let owner = &msgs[1].author;
        assert!(owner.owner && !owner.moderator);

        let member = &msgs[2].author;
        assert!(member.moderator && member.member && member.verified);
        assert_eq!(member.member_badge.as_deref(), Some("https://yt3.ggpht.com/badge-32"));
        assert_eq!(member.member_label.as_deref(), Some("Member (6 bulan)"));
        assert_eq!(msgs[2].runs, vec![Run::Text { text: "cek link https://contoh.id ya".into() }]);
    }

    #[test]
    fn paid_and_membership_items() {
        let page = parse_chat_page(&fixture("chat_events.json")).unwrap();
        let msgs = upserts(&page);
        let kinds: Vec<MessageKind> = msgs.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MessageKind::SuperChat,
                MessageKind::SuperSticker,
                MessageKind::Membership,
                MessageKind::Membership,
                MessageKind::GiftPurchase,
                MessageKind::GiftRedemption,
                MessageKind::Text,
            ]
        );

        let sc = msgs[0];
        assert_eq!(sc.amount.as_deref(), Some("Rp 50.000"));
        let colors = sc.colors.as_ref().unwrap();
        assert_eq!(colors.header_bg, "rgba(30, 136, 229, 1)");
        assert_eq!(colors.body_text, "rgba(255, 255, 255, 1)");
        assert_eq!(sc.runs, vec![Run::Text { text: "semangat bang!".into() }]);

        let sticker = msgs[1];
        assert_eq!(sticker.amount.as_deref(), Some("Rp 20.000"));
        assert_eq!(sticker.sticker.as_deref(), Some("https://lh3.googleusercontent.com/stiker-80"));
        assert_eq!(sticker.header.as_deref(), Some("Kucing menari"));

        assert_eq!(msgs[2].header.as_deref(), Some("Selamat datang di Kanal Uji!"));
        assert!(msgs[2].runs.is_empty());
        assert_eq!(msgs[3].header.as_deref(), Some("Member selama 6 bulan"));
        assert_eq!(msgs[3].runs, vec![Run::Text { text: "sudah setengah tahun!".into() }]);

        let gift = msgs[4];
        assert_eq!(gift.author.name, "@dermawan");
        assert_eq!(gift.author.channel_id, "UCgggggggggggggggggggggg");
        assert_eq!(gift.header.as_deref(), Some("Menghadiahkan 5 membership Kanal Uji"));

        assert_eq!(
            msgs[5].runs,
            vec![Run::Text { text: "menerima membership hadiah dari @dermawan".into() }]
        );

        // Aksi replace: placeholder digantikan pesan final.
        let replace = page.actions.iter().find_map(|a| match a {
            ChatAction::Upsert { replaces: Some(target), message } => Some((target, message)),
            _ => None,
        });
        let (target, replaced) = replace.expect("ada aksi replace");
        assert_eq!(target, "PLACEHOLDER_1");
        assert_eq!(replaced.id, "MSG_FINAL_1");

        assert!(page.actions.contains(&ChatAction::Remove("MSG_DIHAPUS_1".into())));
        assert!(page.actions.contains(&ChatAction::Remove("MSG_DIHAPUS_2".into())));
        assert!(page.actions.contains(&ChatAction::RemoveByAuthor("UCbanbanbanbanbanbanbanb".into())));
        assert_eq!(page.next.as_ref().map(|c| c.timeout_ms), Some(Some(5000)));
        assert_eq!(page.live_chat_token, None);
    }

    #[test]
    fn ended_or_invalid_pages() {
        let ended = json!({ "continuationContents": { "liveChatContinuation": { "actions": [] } } });
        assert_eq!(parse_chat_page(&ended).unwrap().next, None);
        assert_eq!(parse_chat_page(&json!({ "responseContext": {} })), Err(PageError::NoChat));
    }

    #[test]
    fn malformed_items_are_skipped() {
        let page = json!({ "continuationContents": { "liveChatContinuation": { "actions": [
            { "addChatItemAction": { "item": { "liveChatTextMessageRenderer": { "message": { "runs": [{ "text": "tanpa id" }] } } } } },
            { "addChatItemAction": { "item": { "liveChatPaidMessageRenderer": { "id": "x", "purchaseAmountText": { "simpleText": "$1" } } } } },
            { "addChatItemAction": { "item": { "liveChatTextMessageRenderer": { "id": "kosong", "message": { "runs": [] } } } } },
            { "addChatItemAction": { "item": "bukan objek" } },
            { "addChatItemAction": {} },
            { "aksiBaruYangBelumDikenal": { "x": 1 } },
            { "addChatItemAction": { "item": { "liveChatTextMessageRenderer": {
                "id": "ok", "message": { "runs": [{ "emoji": { "emojiId": "UCx/abc", "isCustomEmoji": true } }] } } } } }
        ] } } });
        let page = parse_chat_page(&page).unwrap();
        assert_eq!(page.actions.len(), 1);
        // Emoji kustom tanpa gambar menjadi teks alternatif.
        let msgs = upserts(&page);
        assert_eq!(msgs[0].runs, vec![Run::Text { text: ":emoji:".into() }]);
    }

    #[test]
    fn colors_are_argb() {
        assert_eq!(color(Some(&json!(4_280_191_205u64))), Some("rgba(30, 136, 229, 1)".into()));
        assert_eq!(color(Some(&json!(2_164_260_863u64))), Some("rgba(255, 255, 255, 0.502)".into()));
        assert_eq!(color(Some(&json!("merah"))), None);
        assert_eq!(color(None), None);
    }

    #[test]
    fn next_response() {
        let info = parse_next(&fixture("next_live.json"));
        assert_eq!(info.title.as_deref(), Some("Main ranked malam ini 🎮"));
        assert_eq!(info.channel.as_deref(), Some("Kanal Uji"));
        assert_eq!(info.chat_token.as_deref(), Some("TOKEN_TOP_CHAT"));
        assert!(info.is_live && !info.is_replay);
        assert_eq!(info.viewers, Some(3224));

        let no_chat = parse_next(&json!({ "contents": { "twoColumnWatchNextResults": {} } }));
        assert_eq!(no_chat.chat_token, None);
        assert!(!no_chat.is_live);
    }

    #[test]
    fn viewership() {
        let live = json!({ "actions": [
            { "clickTrackingParams": "x", "updateDateTextAction": {} },
            { "clickTrackingParams": "x", "updateViewershipAction": { "viewCount": { "videoViewCountRenderer": {
                "viewCount": { "simpleText": "3.224 sedang menonton" }, "isLive": true, "originalViewCount": "3224" } } } }
        ] });
        assert_eq!(parse_viewership(&live), Some(Viewership { viewers: Some(3224), is_live: true }));

        let ended = json!({ "actions": [{ "updateViewershipAction": { "viewCount": { "videoViewCountRenderer": {
            "viewCount": { "simpleText": "10 rb x ditonton" }, "originalViewCount": "10021" } } } }] });
        assert_eq!(parse_viewership(&ended), Some(Viewership { viewers: None, is_live: false }));
        assert_eq!(parse_viewership(&json!({})), None);
    }

    #[test]
    fn live_pages() {
        let live = r#"<html><link rel="canonical" href="https://www.youtube.com/watch?v=4t4AQ15yj1M"><script>
            var ytcfg={"INNERTUBE_CLIENT_VERSION":"2.20261112.00.00"};"isLiveNow":true,</script>"#;
        assert_eq!(
            parse_live_page(live),
            LivePage {
                video_id: Some("4t4AQ15yj1M".into()),
                live_now: true,
                upcoming: false,
                client_version: Some("2.20261112.00.00".into()),
            }
        );

        let upcoming = r#"<link rel="canonical" href="https://www.youtube.com/watch?v=abcdefghijk">"isUpcoming":true"#;
        let p = parse_live_page(upcoming);
        assert_eq!(p.video_id.as_deref(), Some("abcdefghijk"));
        assert!(p.upcoming && !p.live_now);

        let offline = r#"<link rel="canonical" href="https://www.youtube.com/channel/UCSJ4gkVC6NrvII8umztf0Ow">"isLiveNow":true"#;
        assert_eq!(parse_live_page(offline).video_id, None);
        assert!(!parse_live_page(offline).live_now);
        assert_eq!(parse_live_page(""), LivePage::default());
    }
}
