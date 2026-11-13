//! Mengubah isian "sumber chat" dari user menjadi video atau channel YouTube (fungsi murni).

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// ID video (11 karakter).
    Video(String),
    /// Path channel tanpa garis miring awal: `@handle`, `channel/UC…`, `c/nama`, atau `user/nama`.
    Channel(String),
}

impl Source {
    /// Halaman yang otomatis mengarah ke live yang sedang berjalan di channel.
    pub fn live_page_url(channel_path: &str) -> String {
        format!("https://www.youtube.com/{channel_path}/live")
    }
}

const HINT: &str =
    "Isi dengan URL video live, URL channel, atau @handle (mis. @namachannel).";

fn is_video_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_channel_id(s: &str) -> bool {
    s.len() == 24
        && s.starts_with("UC")
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Handle YouTube: huruf (termasuk non-Latin), angka, `_`, `-`, `.`; 3–30 karakter.
fn is_handle(s: &str) -> bool {
    let Some(name) = s.strip_prefix('@') else { return false };
    let len = name.chars().count();
    (3..=30).contains(&len) && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn video(id: &str) -> Result<Source, String> {
    if is_video_id(id) {
        Ok(Source::Video(id.to_string()))
    } else {
        Err(format!("ID video \"{id}\" tidak valid. {HINT}"))
    }
}

fn from_url(url: &Url) -> Result<Source, String> {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let segments: Vec<&str> = url.path_segments().map(|s| s.filter(|p| !p.is_empty()).collect()).unwrap_or_default();

    if host == "youtu.be" {
        return segments.first().map_or_else(|| Err(HINT.to_string()), |id| video(id));
    }
    if !matches!(host, "youtube.com" | "m.youtube.com" | "music.youtube.com" | "studio.youtube.com") {
        return Err(format!("Bukan alamat YouTube. {HINT}"));
    }

    match segments.as_slice() {
        ["watch", ..] => url
            .query_pairs()
            .find(|(k, _)| k == "v")
            .map_or_else(|| Err(format!("URL video tanpa parameter v=. {HINT}")), |(_, v)| video(&v)),
        ["live" | "shorts" | "embed" | "video", id, ..] => video(id),
        [handle, ..] if handle.starts_with('@') => {
            // Segment path sudah di-percent-encode oleh parser URL; handle non-Latin perlu didecode.
            let decoded = percent_decode(handle);
            if is_handle(&decoded) {
                Ok(Source::Channel(decoded))
            } else {
                Err(format!("Handle \"{decoded}\" tidak valid. {HINT}"))
            }
        }
        ["channel", id, ..] if is_channel_id(id) => Ok(Source::Channel(format!("channel/{id}"))),
        ["c" | "user", name, ..] => Ok(Source::Channel(format!("{}/{}", segments[0], percent_decode(name)))),
        _ => Err(format!("URL YouTube ini tidak dikenali. {HINT}")),
    }
}

fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        (b as char).to_digit(16).map(|d| d as u8)
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// `input` sudah di-trim dan tidak kosong.
pub fn parse(input: &str) -> Result<Source, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(HINT.to_string());
    }
    if input.starts_with('@') && !input.contains('/') {
        return if is_handle(input) {
            Ok(Source::Channel(input.to_string()))
        } else {
            Err(format!("Handle \"{input}\" tidak valid. {HINT}"))
        };
    }
    if is_video_id(input) {
        return Ok(Source::Video(input.to_string()));
    }
    if is_channel_id(input) {
        return Ok(Source::Channel(format!("channel/{input}")));
    }

    let lower = input.to_ascii_lowercase();
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        let with_scheme = if lower.starts_with("http://") || lower.starts_with("https://") {
            input.to_string()
        } else {
            format!("https://{input}")
        };
        let url = Url::parse(&with_scheme).map_err(|_| format!("URL tidak valid. {HINT}"))?;
        return from_url(&url);
    }
    Err(HINT.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_of(s: &str) -> Source {
        Source::Video(s.into())
    }

    fn channel_of(s: &str) -> Source {
        Source::Channel(s.into())
    }

    #[test]
    fn video_urls() {
        let id = "4t4AQ15yj1M";
        for input in [
            "4t4AQ15yj1M",
            "https://www.youtube.com/watch?v=4t4AQ15yj1M",
            "https://youtube.com/watch?feature=share&v=4t4AQ15yj1M&t=10",
            "youtube.com/watch?v=4t4AQ15yj1M",
            "https://m.youtube.com/watch?v=4t4AQ15yj1M",
            "https://youtu.be/4t4AQ15yj1M?si=abc",
            "https://www.youtube.com/live/4t4AQ15yj1M?feature=shared",
            "https://www.youtube.com/shorts/4t4AQ15yj1M",
            "https://studio.youtube.com/video/4t4AQ15yj1M/livestreaming",
            "  https://www.youtube.com/watch?v=4t4AQ15yj1M  ",
        ] {
            assert_eq!(parse(input), Ok(video_of(id)), "{input}");
        }
    }

    #[test]
    fn channel_inputs() {
        assert_eq!(parse("@LofiGirl"), Ok(channel_of("@LofiGirl")));
        assert_eq!(parse("https://www.youtube.com/@LofiGirl"), Ok(channel_of("@LofiGirl")));
        assert_eq!(parse("youtube.com/@LofiGirl/streams"), Ok(channel_of("@LofiGirl")));
        assert_eq!(parse("https://www.youtube.com/@kanal.saya_1/live"), Ok(channel_of("@kanal.saya_1")));
        assert_eq!(
            parse("https://www.youtube.com/channel/UCSJ4gkVC6NrvII8umztf0Ow"),
            Ok(channel_of("channel/UCSJ4gkVC6NrvII8umztf0Ow"))
        );
        assert_eq!(parse("UCSJ4gkVC6NrvII8umztf0Ow"), Ok(channel_of("channel/UCSJ4gkVC6NrvII8umztf0Ow")));
        assert_eq!(parse("https://www.youtube.com/c/NamaLama"), Ok(channel_of("c/NamaLama")));
        assert_eq!(parse("https://www.youtube.com/user/NamaLama"), Ok(channel_of("user/NamaLama")));
    }

    #[test]
    fn non_latin_handles() {
        assert_eq!(parse("@ゲーム実況"), Ok(channel_of("@ゲーム実況")));
        assert_eq!(parse("https://www.youtube.com/@ゲーム実況"), Ok(channel_of("@ゲーム実況")));
    }

    #[test]
    fn rejects_garbage() {
        for input in [
            "",
            "   ",
            "halo",
            "@a",
            "@nama dengan spasi",
            "https://www.youtube.com/",
            "https://www.youtube.com/watch?x=1",
            "https://www.youtube.com/watch?v=pendek",
            "https://vimeo.com/123",
            "https://www.youtube.com/playlist?list=PL123",
        ] {
            let err = parse(input).expect_err(input);
            assert!(err.contains("@handle"), "{input}: {err}");
        }
    }

    #[test]
    fn live_page_url() {
        assert_eq!(Source::live_page_url("@LofiGirl"), "https://www.youtube.com/@LofiGirl/live");
    }
}
