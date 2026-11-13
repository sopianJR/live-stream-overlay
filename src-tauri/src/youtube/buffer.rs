//! Buffer pesan chat terbaru (fungsi murni). Overlay yang dibuka ulang mengambil isi buffer
//! ini, lalu mengikuti perubahan lewat event `chat://update`.

use std::collections::VecDeque;

use serde::Serialize;

use super::parse::{ChatAction, ChatMessage};

pub const CAPACITY: usize = 200;

/// Perubahan yang dikirim ke frontend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatUpdate {
    /// `true` = buang semua pesan lama dulu (pindah ke live lain).
    pub reset: bool,
    /// Pesan baru, atau pengganti pesan dengan `id` yang sama.
    pub upserts: Vec<ChatMessage>,
    /// ID yang diganti (placeholder) atau dihapus moderator.
    pub removed_ids: Vec<String>,
    /// Channel yang semua pesannya dihapus (di-ban / timeout).
    pub removed_authors: Vec<String>,
}

impl ChatUpdate {
    pub fn is_empty(&self) -> bool {
        !self.reset && self.upserts.is_empty() && self.removed_ids.is_empty() && self.removed_authors.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct ChatBuffer {
    messages: VecDeque<ChatMessage>,
}

impl ChatBuffer {
    pub fn messages(&self) -> Vec<ChatMessage> {
        self.messages.iter().cloned().collect()
    }

    pub fn clear(&mut self) -> ChatUpdate {
        self.messages.clear();
        ChatUpdate { reset: true, ..ChatUpdate::default() }
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.messages.iter().position(|m| m.id == id)
    }

    pub fn apply(&mut self, actions: Vec<ChatAction>) -> ChatUpdate {
        let mut update = ChatUpdate::default();
        for action in actions {
            match action {
                ChatAction::Upsert { replaces, message } => {
                    if let Some(old) = replaces.filter(|old| *old != message.id) {
                        if let Some(i) = self.position(&old) {
                            self.messages.remove(i);
                        }
                        update.upserts.retain(|m| m.id != old);
                        update.removed_ids.push(old);
                    }
                    match self.position(&message.id) {
                        // ID yang sama datang lagi (mis. halaman pertama diambil ulang): ganti di tempat.
                        Some(i) => self.messages[i] = message.clone(),
                        None => self.messages.push_back(message.clone()),
                    }
                    update.upserts.retain(|m| m.id != message.id);
                    update.upserts.push(message);
                }
                ChatAction::Remove(id) => {
                    if let Some(i) = self.position(&id) {
                        self.messages.remove(i);
                    }
                    update.upserts.retain(|m| m.id != id);
                    update.removed_ids.push(id);
                }
                ChatAction::RemoveByAuthor(channel) => {
                    self.messages.retain(|m| m.author.channel_id != channel);
                    update.upserts.retain(|m| m.author.channel_id != channel);
                    update.removed_authors.push(channel);
                }
            }
        }
        while self.messages.len() > CAPACITY {
            self.messages.pop_front();
        }
        update
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::{Author, MessageKind, Run};
    use super::*;

    fn msg(id: &str, author: &str) -> ChatMessage {
        ChatMessage {
            id: id.into(),
            kind: MessageKind::Text,
            author: Author { name: author.into(), channel_id: format!("UC-{author}"), ..Author::default() },
            runs: vec![Run::Text { text: format!("pesan {id}") }],
            timestamp_ms: 0,
            amount: None,
            header: None,
            sticker: None,
            colors: None,
        }
    }

    fn add(id: &str, author: &str) -> ChatAction {
        ChatAction::Upsert { replaces: None, message: msg(id, author) }
    }

    fn ids(buffer: &ChatBuffer) -> Vec<String> {
        buffer.messages().into_iter().map(|m| m.id).collect()
    }

    #[test]
    fn adds_and_caps() {
        let mut b = ChatBuffer::default();
        let actions = (0..CAPACITY + 5).map(|i| add(&i.to_string(), "a")).collect();
        let update = b.apply(actions);
        assert_eq!(update.upserts.len(), CAPACITY + 5);
        assert_eq!(b.messages().len(), CAPACITY);
        assert_eq!(ids(&b).first().map(String::as_str), Some("5"));
    }

    #[test]
    fn duplicate_ids_replace_in_place() {
        let mut b = ChatBuffer::default();
        b.apply(vec![add("1", "a"), add("2", "b")]);
        let update = b.apply(vec![add("1", "a")]);
        assert_eq!(ids(&b), vec!["1", "2"]);
        assert_eq!(update.upserts.len(), 1);
    }

    #[test]
    fn replace_placeholder() {
        let mut b = ChatBuffer::default();
        b.apply(vec![add("p1", "a"), add("2", "b")]);
        let update = b.apply(vec![ChatAction::Upsert { replaces: Some("p1".into()), message: msg("final", "a") }]);
        assert_eq!(ids(&b), vec!["2", "final"]);
        assert_eq!(update.removed_ids, vec!["p1"]);
        assert_eq!(update.upserts[0].id, "final");
    }

    #[test]
    fn removals() {
        let mut b = ChatBuffer::default();
        b.apply(vec![add("1", "a"), add("2", "spammer"), add("3", "b"), add("4", "spammer")]);
        let update = b.apply(vec![
            ChatAction::Remove("1".into()),
            ChatAction::RemoveByAuthor("UC-spammer".into()),
            add("5", "spammer"),
        ]);
        // Pesan setelah ban dalam batch yang sama tetap masuk.
        assert_eq!(ids(&b), vec!["3", "5"]);
        assert_eq!(update.removed_ids, vec!["1"]);
        assert_eq!(update.removed_authors, vec!["UC-spammer"]);
    }

    #[test]
    fn removal_in_same_batch_drops_pending_upsert() {
        let mut b = ChatBuffer::default();
        let update = b.apply(vec![add("1", "a"), ChatAction::Remove("1".into())]);
        assert!(update.upserts.is_empty());
        assert!(b.messages().is_empty());
    }

    #[test]
    fn clear_resets() {
        let mut b = ChatBuffer::default();
        b.apply(vec![add("1", "a")]);
        let update = b.clear();
        assert!(update.reset && !update.is_empty());
        assert!(b.messages().is_empty());
        assert!(ChatUpdate::default().is_empty());
    }
}
