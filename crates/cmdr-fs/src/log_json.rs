//! A serializable value's JSON for a log line, each user-derived field marked by its kind.
//!
//! Some errors log as the very JSON the frontend gets (`WriteOperationError::log_detail`), so
//! a report tokenizes the same keys from either side. That JSON is data and must never carry a
//! marker, so the log form is built beside it: the same serialization, then each field the
//! caller names wrapped as a span of its kind, each [`ErrorDetail`](crate::error_detail::ErrorDetail)
//! written as its own marked log text, and every other string's reserved chars replaced, so
//! someone else's text can't forge a span.
//!
//! **The writer keeps a span's body verbatim.** A span holds its value as the `{:?}` body
//! (`LogPrivate`), and a span at depth d must be escaped exactly d+1 times. JSON escaping it
//! once more would put the body a level deeper than its raw markers, so inside a span the
//! formatter writes `"` and `\` as they are: the body already escaped them, and the result is
//! still JSON (`\"` and `\\` are JSON's own escapes). Outside spans it is plain JSON.

use std::cell::Cell;
use std::fmt::{self, Write as _};
use std::io;

use serde::Serialize;
use serde_json::ser::{CharEscape, Formatter};
use serde_json::{Map, Value};

use crate::log_detail::LoggedText;
use crate::log_private::{CLOSER, LogKind, LogPrivate, is_reserved};

/// What a JSON field holds, for [`log_json`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonField {
    /// A user-derived value of this kind: marked as one span.
    Mark(LogKind),
    /// An `ErrorDetail`, which wrote its own marked log text.
    Detail,
    /// Our own words or a fixed vocabulary (a type tag, an enum's name): text, with any reserved
    /// char replaced.
    Words,
}

/// A value's JSON with its fields marked: log text only, ❌ never data.
#[must_use = "a LoggedJson means something only inside a log macro's arguments"]
pub struct LoggedJson(String);

impl LoggedJson {
    /// The marked JSON, markers raw (depth 0). For tests and for a caller composing its own
    /// field; a log site prints the value with `{:?}`.
    pub fn as_marked_text(&self) -> &str {
        &self.0
    }
}

/// Quoted and capped as one field, markers kept (`LoggedText`), so its spans sit at depth 1.
impl fmt::Debug for LoggedJson {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        LoggedText(&self.0).fmt(f)
    }
}

thread_local! {
    static MARKING: Cell<bool> = const { Cell::new(false) };
}

/// Whether the current thread is serializing for [`log_json`], which is when an `ErrorDetail`
/// writes its marked log text instead of its plain one.
pub(crate) fn is_marking() -> bool {
    MARKING.with(Cell::get)
}

/// Resets the flag however the serialization ends.
struct MarkingScope;

impl MarkingScope {
    fn enter() -> Self {
        MARKING.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for MarkingScope {
    fn drop(&mut self) {
        MARKING.with(|flag| flag.set(false));
    }
}

/// `value`'s JSON for a log line: each field `kind_of` names is marked by its kind (an array
/// takes its key's kind for each element), every other string is [`JsonField::Words`]'s.
pub fn log_json<T: Serialize + ?Sized>(value: &T, kind_of: impl Fn(&str) -> JsonField) -> LoggedJson {
    let tree = {
        let _scope = MarkingScope::enter();
        serde_json::to_value(value)
    };
    let tree = match tree {
        Ok(tree) => mark(tree, JsonField::Words, &kind_of),
        Err(e) => return LoggedJson(format!("(the value didn't serialize: {e})")),
    };
    let mut out = Vec::new();
    let mut writer = serde_json::Serializer::with_formatter(&mut out, SpanFormatter::default());
    if let Err(e) = tree.serialize(&mut writer) {
        return LoggedJson(format!("(the value didn't serialize: {e})"));
    }
    // The formatter writes only what `serde_json` handed it plus whole chars: always UTF-8.
    LoggedJson(String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

fn mark(value: Value, field: JsonField, kind_of: &impl Fn(&str) -> JsonField) -> Value {
    match value {
        Value::String(text) => Value::String(match field {
            JsonField::Mark(kind) => marked(kind, &text),
            JsonField::Detail => text,
            JsonField::Words => text
                .chars()
                .map(|c| if is_reserved(c) { '\u{FFFD}' } else { c })
                .collect(),
        }),
        Value::Array(items) => Value::Array(items.into_iter().map(|item| mark(item, field, kind_of)).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| {
                    let kind = kind_of(&key);
                    (key, mark(value, kind, kind_of))
                })
                .collect::<Map<_, _>>(),
        ),
        other => other,
    }
}

/// `text` as one span of `kind`, written as a log line would.
fn marked(kind: LogKind, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    // A log field's text, which reaches a log macro only through `LoggedJson`'s `Debug`.
    let _ = write!(out, "{}", LogPrivate::new(kind, text));
    out
}

/// Plain JSON outside spans; inside one, the body's own escapes pass through verbatim.
// DEFAULT-OK: a string starts outside any span.
#[derive(Default)]
struct SpanFormatter {
    in_span: bool,
}

impl Formatter for SpanFormatter {
    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()> {
        for c in fragment.chars() {
            if LogKind::from_opener(c).is_some() {
                self.in_span = true;
            } else if c == CLOSER {
                self.in_span = false;
            }
        }
        writer.write_all(fragment.as_bytes())
    }

    fn write_char_escape<W: ?Sized + io::Write>(&mut self, writer: &mut W, escape: CharEscape) -> io::Result<()> {
        if !self.in_span {
            return serde_json::ser::CompactFormatter.write_char_escape(writer, escape);
        }
        // A span body is a `{:?}` body: `"` and `\` arrive already escaped, and it holds no
        // raw control char, so these two are the only escapes that can reach here.
        let raw: &[u8] = match escape {
            CharEscape::Quote => b"\"",
            CharEscape::ReverseSolidus => b"\\",
            other => return serde_json::ser::CompactFormatter.write_char_escape(writer, other),
        };
        writer.write_all(raw)
    }
}

#[cfg(test)]
#[path = "log_json_test.rs"]
mod tests;
