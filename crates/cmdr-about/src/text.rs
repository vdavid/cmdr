//! Text an item says about itself, made safe to show and to send.
//!
//! A file's name, its download record, its `Info.plist` strings, a signer's name,
//! and a branch name are written by whoever made the item. [`ItemText`] is the only
//! way such a string gets into a fact, so a card row and the model's projection can't
//! receive one that skipped [`model_text`].

use serde::Serialize;

/// The longest an item's own string gets on a card row or in the projection.
/// Longer text keeps its start and its end (where an extension lives).
pub const MAX_ITEM_TEXT_CHARS: usize = 400;

/// Whether `c` could make text read as something it isn't: a control character, a
/// bidirectional mark or override (`cod\u{202E}fdp.command` shows a person a PDF),
/// or a line or paragraph separator that would start a row of its own.
fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

/// `text` as plain, bounded text: no control or bidirectional-override characters,
/// and the middle dropped past [`MAX_ITEM_TEXT_CHARS`].
pub fn model_text(text: &str) -> String {
    let plain: Vec<char> = text.chars().filter(|c| !is_unsafe_char(*c)).collect();
    if plain.len() <= MAX_ITEM_TEXT_CHARS {
        return plain.into_iter().collect();
    }
    let keep = MAX_ITEM_TEXT_CHARS / 2;
    let head: String = plain[..keep].iter().collect();
    let tail: String = plain[plain.len() - keep..].iter().collect();
    format!("{head}…{tail}")
}

/// A string the item reported about itself, already through [`model_text`].
///
/// It's a claim, never a checked fact: render it as a text node and word it as
/// recorded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(transparent)]
pub struct ItemText(String);

impl ItemText {
    /// Sanitizes `raw` and wraps it.
    pub fn new(raw: &str) -> Self {
        Self(model_text(raw))
    }

    /// The sanitized text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_plain_text_is_kept_markup_included() {
        assert_eq!(model_text("report <b>final</b>.pdf"), "report <b>final</b>.pdf");
    }

    #[test]
    fn what_would_make_a_name_read_as_something_else_is_dropped() {
        assert_eq!(model_text("cod\u{202E}fdp.command"), "codfdp.command");
        assert_eq!(model_text("a\nb\u{2066}c\u{2028}d\u{061C}e\u{0}"), "abcde");
    }

    #[test]
    fn long_text_keeps_its_end_where_the_extension_lives() {
        let long = format!("{}.command", "a".repeat(10_000));
        let shown = model_text(&long);
        assert_eq!(shown.chars().count(), MAX_ITEM_TEXT_CHARS + 1);
        assert!(shown.ends_with(".command"));
        assert!(shown.contains('…'));
    }

    #[test]
    fn item_text_crosses_the_wire_as_a_bare_string() {
        let text = ItemText::new("Sla\u{202E}ck");
        assert_eq!(text.as_str(), "Slack");
        assert_eq!(serde_json::to_string(&text).expect("a string serializes"), "\"Slack\"");
    }
}
