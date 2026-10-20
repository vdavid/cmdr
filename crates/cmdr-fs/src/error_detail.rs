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

use crate::log_error::{ErrorLogText, ErrorText};

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

/// The plain text, quoted, exactly as the `String` this replaces printed.
impl fmt::Debug for ErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_string(), f)
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
