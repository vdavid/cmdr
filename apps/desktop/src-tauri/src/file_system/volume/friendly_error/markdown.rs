//! `Markdown` newtype + `md!` macro: typed markdown strings with automatic
//! escaping of interpolated runtime values.
//!
//! ## Why
//!
//! `FriendlyError.explanation` and `.suggestion` are rendered on the frontend
//! via `snarkdown`. Before this module, both fields were `String`, and call
//! sites used plain `format!("...{}...", runtime_value)`. Runtime values like
//! OS error messages can contain markdown metacharacters (the wild
//! `STATUS_DELETE_PENDING` rendered as `STATUS<em>DELETE</em>PENDING`).
//!
//! ## How
//!
//! - `Markdown(String)` is the typed wrapper. Serializes transparently as a
//!   JSON string so the wire format is unchanged.
//! - `md!("template", arg, arg, ...)` works like `format!` but each `{}` arg
//!   must implement `MarkdownArg`. Plain `&str` / `String` / `Path` escape
//!   automatically; a `Markdown` value passes through unescaped. There is no
//!   way to interpolate a raw runtime string without going through one of
//!   those paths.
//! - `Markdown::literal(s)` wraps an already-trusted string (typically a
//!   static literal). The author asserts the content is safe markdown.
//!
//! ## Escaping
//!
//! The escaper is conservative: it backslash-escapes every CommonMark
//! punctuation character. The result still renders as the same plain text
//! after parsing, just without any accidental formatting.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;
use std::path::{Display as PathDisplay, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(transparent)]
pub struct Markdown(String);

impl Markdown {
    /// Wrap a string that is already trusted markdown (typically a static
    /// literal in source code, or the output of `md!`). No escaping happens.
    pub fn literal(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Markdown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Implemented by every value `md!` can interpolate. Untrusted strings escape;
/// `Markdown` values pass through unescaped.
pub trait MarkdownArg {
    fn render_arg(&self) -> Cow<'_, str>;
}

impl MarkdownArg for str {
    fn render_arg(&self) -> Cow<'_, str> {
        escape(self)
    }
}

impl MarkdownArg for String {
    fn render_arg(&self) -> Cow<'_, str> {
        escape(self)
    }
}

impl MarkdownArg for Cow<'_, str> {
    fn render_arg(&self) -> Cow<'_, str> {
        escape(self.as_ref())
    }
}

impl MarkdownArg for PathDisplay<'_> {
    fn render_arg(&self) -> Cow<'_, str> {
        Cow::Owned(escape(&self.to_string()).into_owned())
    }
}

impl MarkdownArg for Path {
    fn render_arg(&self) -> Cow<'_, str> {
        Cow::Owned(escape(&self.display().to_string()).into_owned())
    }
}

impl MarkdownArg for PathBuf {
    fn render_arg(&self) -> Cow<'_, str> {
        self.as_path().render_arg()
    }
}

impl MarkdownArg for Markdown {
    fn render_arg(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.0)
    }
}

impl<T: MarkdownArg + ?Sized> MarkdownArg for &T {
    fn render_arg(&self) -> Cow<'_, str> {
        (*self).render_arg()
    }
}

// Numeric impls: integers don't contain markdown specials so they can
// short-circuit the escape pass. Added on demand as call sites require them.
macro_rules! impl_markdown_arg_for_number {
    ($($t:ty),+) => {
        $(
            impl MarkdownArg for $t {
                fn render_arg(&self) -> Cow<'_, str> {
                    Cow::Owned(self.to_string())
                }
            }
        )+
    };
}
impl_markdown_arg_for_number!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

/// Encode markdown-meaningful characters as HTML numeric entities so they
/// pass through snarkdown without triggering formatting, then render as
/// the original characters in the browser.
///
/// **Why entities and not CommonMark `\` escapes**: snarkdown is a tiny
/// non-CommonMark parser. It does NOT honor backslash escapes — emitting
/// `STATUS\_DELETE\_PENDING` would render visibly with backslashes. HTML
/// entities sidestep snarkdown entirely (it doesn't recognize them), and
/// the browser decodes them when the result is `{@html}`-injected.
///
/// `&` must be encoded first so any preexisting entity-like text is
/// neutralized.
fn escape(s: &str) -> Cow<'_, str> {
    let needs_escape = s.chars().any(|c| c == '&' || is_md_special(c));
    if !needs_escape {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' => out.push_str("&#92;"),
            '`' => out.push_str("&#96;"),
            '*' => out.push_str("&#42;"),
            '_' => out.push_str("&#95;"),
            '[' => out.push_str("&#91;"),
            ']' => out.push_str("&#93;"),
            '(' => out.push_str("&#40;"),
            ')' => out.push_str("&#41;"),
            '!' => out.push_str("&#33;"),
            '~' => out.push_str("&#126;"),
            _ => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// snarkdown characters that are meaningful regardless of position in a
/// line. We intentionally do NOT escape `.`, `-`, `+`, `#`, `|` — they
/// only have markdown meaning at the start of a line, and over-escaping
/// them shows up as ugly entities mid-sentence. Runtime values land
/// mid-sentence in our templates, so line-start chars stay innocuous.
const fn is_md_special(c: char) -> bool {
    matches!(
        c,
        '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '!' | '<' | '>' | '~'
    )
}

/// `md!("template")` and `md!("template {} {}", arg, arg)` build a `Markdown`.
/// Template literals are treated as trusted markdown; each `{}` arg goes
/// through `MarkdownArg::render_arg` (escapes plain strings, passes
/// `Markdown` through).
///
/// **Use positional `{}` placeholders only.** Captured-identifier syntax
/// (`md!("foo {bar}")` with `bar` in scope) bypasses the escape machinery
/// — the no-args arm doesn't run `format!`, so the literal `{bar}` would
/// render verbatim in the UI. Always pass interpolated values as explicit
/// positional args: `md!("foo {}", bar)`.
#[macro_export]
macro_rules! md {
    ($lit:literal) => {
        $crate::file_system::volume::friendly_error::Markdown::literal($lit)
    };
    ($lit:literal, $($arg:expr),+ $(,)?) => {
        $crate::file_system::volume::friendly_error::Markdown::literal(format!(
            $lit
            $(, $crate::file_system::volume::friendly_error::MarkdownArg::render_arg(&$arg))+
        ))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_is_not_escaped() {
        let m = Markdown::literal("Bold: **yes** and `code`");
        assert_eq!(m.as_str(), "Bold: **yes** and `code`");
    }

    #[test]
    fn escape_passes_through_plain_text() {
        assert!(matches!(escape("plain text"), Cow::Borrowed(_)));
        assert_eq!(escape("plain text"), "plain text");
    }

    #[test]
    fn escape_encodes_markdown_specials_as_html_entities() {
        // snarkdown doesn't parse `\_` as an escape so we use entities; the
        // browser decodes them at render time.
        assert_eq!(escape("STATUS_DELETE_PENDING"), "STATUS&#95;DELETE&#95;PENDING");
        assert_eq!(escape("**bold**"), "&#42;&#42;bold&#42;&#42;");
        assert_eq!(escape("[link](url)"), "&#91;link&#93;&#40;url&#41;");
        assert_eq!(escape("a `code` span"), "a &#96;code&#96; span");
    }

    #[test]
    fn escape_neutralizes_preexisting_entities() {
        // `&amp;` is the only safe way to ship a literal `&` through snarkdown
        // and then through `{@html}`. Any `&` (including ones that already look
        // like entities) is encoded so the original characters survive.
        assert_eq!(escape("a & b"), "a &amp; b");
        assert_eq!(escape("&lt;script&gt;"), "&amp;lt;script&amp;gt;");
    }

    #[test]
    fn escape_leaves_line_start_chars_alone() {
        // `.`, `-`, `+`, `#`, `|` only have markdown meaning at line start, and
        // runtime values land mid-sentence. Leaving them unescaped keeps the
        // rendered output readable.
        assert_eq!(escape("Sync.com"), "Sync.com");
        assert_eq!(escape("a-dashed-path"), "a-dashed-path");
        assert_eq!(escape("photo.jpg"), "photo.jpg");
    }

    #[test]
    fn md_macro_literal_only() {
        let m = md!("Hello world");
        assert_eq!(m.as_str(), "Hello world");
    }

    #[test]
    fn md_macro_escapes_string_args() {
        let message = "Protocol error: STATUS_DELETE_PENDING during Create";
        let m = md!("Hit a problem: {}.", message);
        assert_eq!(
            m.as_str(),
            "Hit a problem: Protocol error: STATUS&#95;DELETE&#95;PENDING during Create."
        );
    }

    #[test]
    fn md_macro_escapes_path_display() {
        let path = Path::new("/Volumes/naspi/_todo_pics/file.jpg");
        let m = md!("Path: `{}`.", path.display());
        // Underscores encoded as entities; `.` and `/` left alone.
        assert_eq!(m.as_str(), "Path: `/Volumes/naspi/&#95;todo&#95;pics/file.jpg`.");
    }

    #[test]
    fn md_macro_passes_markdown_arg_through_unescaped() {
        let bold = Markdown::literal("**MacDroid**");
        let m = md!("Detected provider: {}.", bold);
        assert_eq!(m.as_str(), "Detected provider: **MacDroid**.");
    }

    #[test]
    fn md_macro_mixes_escaped_and_literal_args() {
        let provider = Markdown::literal("**MacDroid**");
        let raw_path = "_my_folder";
        let m = md!("{} can't read `{}`.", provider, raw_path);
        assert_eq!(m.as_str(), "**MacDroid** can't read `&#95;my&#95;folder`.");
    }

    #[test]
    fn serde_is_transparent() {
        let m = Markdown::literal("**hello**");
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#""**hello**""#);
        let back: Markdown = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
