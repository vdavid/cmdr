//! Log an error whose fields hold paths and names, with each of them marked.
//!
//! An error type that carries a path writes its text ONCE, through an [`ErrorText`]
//! ([`ErrorLogText::write_log_text`]). Its `Display` calls that with a plain writer, so the text
//! a person, a store, or IPC ever sees carries no marker; [`LogError`] calls it with a marking
//! writer, so a log line hashes each path exactly instead of guessing where it ends. One
//! function behind both means the two texts can't drift: strip the markers from the logged form
//! and it reads exactly as `Display`.
//!
//! Log a converted error as `{}` with `LogError(&e)` (or `{:?}`, which quotes it). An error that
//! doesn't implement [`ErrorLogText`] doesn't compile there, so a site can't think it marked
//! something it didn't. An error flattened into a `String` before it reaches a log macro (a
//! `format!` into a message) prints plain, and the report's regex fallback is all it gets: a
//! message field that wraps another error holds a
//! [`ErrorDetail`](crate::error_detail::ErrorDetail) instead, which keeps it typed.

use crate::log_private::{DebugEscaping, LogPrivate, is_reserved};
use std::fmt::{self, Write as _};

/// An error that writes its text through an [`ErrorText`], marking each user-derived value it
/// carries as what it is. Its `Display` should be [`write_plain`] of this, so the plain and the
/// marked texts are one function.
pub trait ErrorLogText {
    /// Write the error's text: its own words through `write!`/`write_str`, and each path, name,
    /// share, or host through [`ErrorText::value`].
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result;
}

/// A borrowed error writes what it borrows, so a site holding `&e` logs `LogError(e)` or
/// `LogError(&e)` alike.
impl<T: ErrorLogText + ?Sized> ErrorLogText for &T {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result {
        (**self).write_log_text(out)
    }
}

/// Where an [`ErrorLogText`] writes: plain for `Display`, marking for [`LogError`].
pub struct ErrorText<'w> {
    out: &'w mut dyn fmt::Write,
    marking: bool,
}

impl ErrorText<'_> {
    /// Write a user-derived value: as itself in plain text, marked as its kind in a log line. An
    /// empty value writes nothing either way: there's no name in it to hash.
    pub fn value(&mut self, value: LogPrivate<'_>) -> fmt::Result {
        let text = value.text();
        if text.is_empty() {
            Ok(())
        } else if self.marking {
            write!(self.out, "{value}")
        } else {
            self.out.write_str(&text)
        }
    }

    /// Write a user-derived value as its `{:?}` spelling: quoted and escaped in plain text,
    /// the same quotes around its span in a log line, so the two still differ only by markers.
    pub fn quoted_value(&mut self, value: LogPrivate<'_>) -> fmt::Result {
        if self.marking {
            write!(self.out, "{value:?}")
        } else {
            write!(self.out, "{:?}", value.text())
        }
    }

    /// Write a nested error that marks its own values, the same way this one is written.
    pub fn error<E: ErrorLogText + ?Sized>(&mut self, inner: &E) -> fmt::Result {
        inner.write_log_text(self)
    }
}

/// The error's own words, and any foreign text it carries (an OS or server message). In a log
/// line every reserved char in it prints as U+FFFD: only the error's own marks are legitimate,
/// and a hostile name echoed in a server's message would otherwise forge one.
impl fmt::Write for ErrorText<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if !self.marking {
            return self.out.write_str(s);
        }
        for (index, piece) in s.split(is_reserved).enumerate() {
            if index > 0 {
                self.out.write_char('\u{FFFD}')?;
            }
            self.out.write_str(piece)?;
        }
        Ok(())
    }
}

/// An [`ErrorLogText`]'s plain text, for its `Display`: no marker, ever.
pub fn write_plain<E: ErrorLogText + ?Sized>(error: &E, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    error.write_log_text(&mut ErrorText { out: f, marking: false })
}

/// An error in a log line, with each path and name it carries marked as its kind.
///
/// `{}` writes the error's `Display` text with the values marked; `{:?}` writes the same quoted
/// and escaped as one field (its spans one depth down, which the report pass reads natively).
/// ❌ Like every marked value, it's log text only: never a `String`, a message, or UI copy.
#[must_use = "a LogError means something only inside a log macro's arguments"]
pub struct LogError<'a, E: ErrorLogText + ?Sized>(pub &'a E);

impl<E: ErrorLogText + ?Sized> fmt::Display for LogError<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write_log_text(&mut ErrorText { out: f, marking: true })
    }
}

impl<E: ErrorLogText + ?Sized> fmt::Debug for LogError<'_, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        write!(DebugEscaping(f), "{self}")?;
        f.write_char('"')
    }
}

#[cfg(test)]
#[path = "log_error_test.rs"]
mod tests;
