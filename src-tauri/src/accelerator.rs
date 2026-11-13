//! Normalisasi & validasi teks hotkey ("ctrl+shift+o" → "Ctrl+Shift+O").
//!
//! Bentuk tampilan disimpan di pengaturan; saat didaftarkan diubah ke format
//! yang dipahami crate `global-hotkey` (mis. "Win" → "Super").

use tauri_plugin_global_shortcut::Shortcut;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl Modifier {
    fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Self::Ctrl),
            "alt" | "option" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            "win" | "super" | "meta" | "cmd" | "command" => Some(Self::Win),
            _ => None,
        }
    }

    fn display(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Win => "Win",
        }
    }

    fn parse_name(self) -> &'static str {
        match self {
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::Shift => "shift",
            Self::Win => "super",
        }
    }
}

/// "KeyO"/"o" → "O", "Digit1" → "1", "f9" → "F9", "space" → "Space".
fn normalize_key(token: &str) -> String {
    let upper = token.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("KEY") {
        if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_alphabetic()) {
            return rest.to_string();
        }
    }
    if let Some(rest) = upper.strip_prefix("DIGIT") {
        if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }
    if token.len() == 1 {
        return upper;
    }
    if upper.starts_with('F') && upper[1..].chars().all(|c| c.is_ascii_digit()) {
        return upper;
    }
    let mut chars = token.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

struct Parsed {
    modifiers: Vec<Modifier>,
    key: String,
}

fn split(input: &str) -> Result<Parsed, String> {
    let mut modifiers = Vec::new();
    let mut key: Option<String> = None;

    for token in input.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        if let Some(m) = Modifier::parse(token) {
            if !modifiers.contains(&m) {
                modifiers.push(m);
            }
        } else if key.is_some() {
            return Err(format!("Hotkey \"{input}\" berisi lebih dari satu tombol utama."));
        } else {
            key = Some(normalize_key(token));
        }
    }

    let key = key.ok_or_else(|| format!("Hotkey \"{input}\" belum berisi tombol utama."))?;
    if modifiers.is_empty() {
        return Err(format!(
            "Hotkey \"{input}\" harus memakai minimal satu modifier (Ctrl, Alt, Shift, atau Win) \
             agar tidak bentrok dengan tombol game."
        ));
    }
    modifiers.sort();
    Ok(Parsed { modifiers, key })
}

fn to_parse_string(p: &Parsed) -> String {
    let mut parts: Vec<&str> = p.modifiers.iter().map(|m| m.parse_name()).collect();
    parts.push(&p.key);
    parts.join("+")
}

/// Merapikan teks hotkey menjadi bentuk tampilan baku, sekaligus memastikan bisa didaftarkan.
pub fn normalize(input: &str) -> Result<String, String> {
    let parsed = split(input)?;
    to_parse_string(&parsed)
        .parse::<Shortcut>()
        .map_err(|_| format!("Tombol \"{}\" pada hotkey \"{input}\" tidak dikenali.", parsed.key))?;
    let mut parts: Vec<&str> = parsed.modifiers.iter().map(|m| m.display()).collect();
    parts.push(&parsed.key);
    Ok(parts.join("+"))
}

/// Mengubah teks hotkey (bentuk tampilan) menjadi `Shortcut` yang siap didaftarkan.
pub fn parse(input: &str) -> Result<Shortcut, String> {
    let parsed = split(input)?;
    to_parse_string(&parsed)
        .parse::<Shortcut>()
        .map_err(|_| format!("Tombol \"{}\" pada hotkey \"{input}\" tidak dikenali.", parsed.key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_order_and_aliases() {
        assert_eq!(normalize("ctrl+shift+o").unwrap(), "Ctrl+Shift+O");
        assert_eq!(normalize(" Shift + Control + KeyE ").unwrap(), "Ctrl+Shift+E");
        assert_eq!(normalize("Ctrl+Shift+Digit1").unwrap(), "Ctrl+Shift+1");
        assert_eq!(normalize("alt+f9").unwrap(), "Alt+F9");
        assert_eq!(normalize("Super+Alt+Backquote").unwrap(), "Alt+Win+Backquote");
        assert_eq!(normalize("ctrl+space").unwrap(), "Ctrl+Space");
    }

    #[test]
    fn rejects_missing_modifier() {
        assert!(normalize("O").unwrap_err().contains("modifier"));
        assert!(normalize("F9").is_err());
    }

    #[test]
    fn rejects_missing_or_multiple_keys() {
        assert!(normalize("Ctrl+Shift").unwrap_err().contains("tombol utama"));
        assert!(normalize("Ctrl+O+P").unwrap_err().contains("lebih dari satu"));
        assert!(normalize("").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(normalize("Ctrl+Blabla").unwrap_err().contains("tidak dikenali"));
    }

    #[test]
    fn parse_matches_equivalent_spellings() {
        assert_eq!(parse("Ctrl+Shift+O").unwrap(), parse("shift+ctrl+KeyO").unwrap());
        assert_ne!(parse("Ctrl+Shift+O").unwrap(), parse("Ctrl+Shift+E").unwrap());
    }
}
