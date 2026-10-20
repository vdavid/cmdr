//! The diagnostic text an error carries for its technical-details panel, kept typed when it came
//! from another error, so a log line still marks the paths and names inside it.
//!
//! An error that wraps another one's text (a viewer's `Io { message }`, a volume's
//! `IoError { message }`) used to hold `cause.to_string()`: plain from then on, so its log line
//! shipped the cause's path through the report's regex fallback only. An [`ErrorDetail`] holds
//! the cause itself. Everywhere a person, a store, or IPC sees it, it IS the cause's `Display`
//! (the JSON is that string, and so is `Debug`); in a log line it writes through the cause's
//! [`ErrorLogText`], marking each value.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::log_error::{ErrorLogText, ErrorText, write_plain};
use crate::log_private::LogPrivate;

/// Diagnostic text for an error's technical details: a backend's or the OS's own words, or a
/// typed error it was built from. Crosses IPC and storage as a plain string.
#[derive(Clone)]
pub struct ErrorDetail(Inner);

#[derive(Clone)]
enum Inner {
    /// Foreign words: what the OS, a server, or a third-party crate said.
    Text(String),
    /// An error of ours, which marks its own values in a log line.
    Error(Arc<dyn Cause>),
}

/// An error an [`ErrorDetail`] can hold: one that writes its own log text.
trait Cause: ErrorLogText + fmt::Display + Send + Sync {}

impl<T: ErrorLogText + fmt::Display + Send + Sync> Cause for T {}

impl ErrorDetail {
    /// The detail of a typed error, kept typed so a log line marks its values.
    pub fn of(cause: impl ErrorLogText + fmt::Display + Send + Sync + 'static) -> Self {
        Self(Inner::Error(Arc::new(cause)))
    }

    /// A backend's own words about a path it names (`"ENOSPC on "` + the path): the words stay
    /// text, the path is marked.
    pub fn at_path(words: impl Into<String>, path: impl Into<String>) -> Self {
        Self::of(AtPath {
            before: words.into(),
            path: path.into(),
            after: String::new(),
        })
    }

    /// The same with the path first (`"<key>: the server ended the upload"`).
    pub fn path_then(path: impl Into<String>, words: impl Into<String>) -> Self {
        Self::of(AtPath {
            before: String::new(),
            path: path.into(),
            after: words.into(),
        })
    }

    /// Words on both sides of the path (`"the write of "` + the path + `" stopped moving"`).
    pub fn around(before: impl Into<String>, path: impl Into<String>, after: impl Into<String>) -> Self {
        Self::of(AtPath {
            before: before.into(),
            path: path.into(),
            after: after.into(),
        })
    }

    /// An `io::Error`'s detail. One built as `io::Error::other(detail)` (a byte source that
    /// speaks only `io` handing its typed failure through) comes back as that detail, values
    /// still marked; any other is the OS's words.
    pub fn from_io(err: &std::io::Error) -> Self {
        match err.get_ref().and_then(|inner| inner.downcast_ref::<Self>()) {
            Some(carried) => carried.clone(),
            None => err.to_string().into(),
        }
    }

    /// A detail that is only the path a backend refused, marked.
    pub fn path(path: impl Into<String>) -> Self {
        Self::at_path(String::new(), path)
    }
}

/// [`ErrorDetail::at_path`]'s and [`ErrorDetail::path_then`]'s cause: a backend's words around
/// the one path they're about.
struct AtPath {
    before: String,
    path: String,
    after: String,
}

impl fmt::Display for AtPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_plain(self, f)
    }
}

impl ErrorLogText for AtPath {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result {
        fmt::Write::write_str(out, &self.before)?;
        out.value(LogPrivate::path(&self.path))?;
        fmt::Write::write_str(out, &self.after)
    }
}

impl ErrorDetail {
    /// The plain text, borrowed when it's already words.
    fn text(&self) -> std::borrow::Cow<'_, str> {
        match &self.0 {
            Inner::Text(text) => std::borrow::Cow::Borrowed(text),
            Inner::Error(cause) => std::borrow::Cow::Owned(cause.to_string()),
        }
    }
}

impl From<String> for ErrorDetail {
    fn from(text: String) -> Self {
        Self(Inner::Text(text))
    }
}

impl From<&str> for ErrorDetail {
    fn from(text: &str) -> Self {
        Self(Inner::Text(text.to_string()))
    }
}

/// The plain text: the cause's `Display`, or the words themselves.
impl fmt::Display for ErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Inner::Text(text) => f.write_str(text),
            Inner::Error(cause) => fmt::Display::fmt(cause, f),
        }
    }
}

/// So a detail can ride inside an `io::Error` ([`ErrorDetail::from_io`]).
impl std::error::Error for ErrorDetail {}

/// The plain text, quoted, exactly as the `String` this replaces printed.
impl fmt::Debug for ErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_string(), f)
    }
}

/// Two details are equal when they read the same: the text is what a person, a store, or IPC
/// ever compares.
impl PartialEq for ErrorDetail {
    fn eq(&self, other: &Self) -> bool {
        self.text() == other.text()
    }
}

impl Eq for ErrorDetail {}

impl PartialEq<str> for ErrorDetail {
    fn eq(&self, other: &str) -> bool {
        self.text() == other
    }
}

impl PartialEq<String> for ErrorDetail {
    fn eq(&self, other: &String) -> bool {
        self.text() == other.as_str()
    }
}

impl PartialEq<&str> for ErrorDetail {
    fn eq(&self, other: &&str) -> bool {
        self.text() == *other
    }
}

/// Foreign words write as text (their reserved chars can't forge a marker); a typed cause writes
/// its own log text, values marked.
impl ErrorLogText for ErrorDetail {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result {
        match &self.0 {
            Inner::Text(text) => fmt::Write::write_str(out, text),
            Inner::Error(cause) => out.error(cause.as_ref()),
        }
    }
}

impl Serialize for ErrorDetail {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A stored detail comes back as text: its words survive, its cause's type doesn't.
impl<'de> Deserialize<'de> for ErrorDetail {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

impl specta::Type for ErrorDetail {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <String as specta::Type>::definition(types)
    }
}

#[cfg(test)]
#[path = "error_detail_test.rs"]
mod tests;
