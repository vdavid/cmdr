//! The verdict's short look inside a file: ONE `inspect_file` core call, kept small.
//!
//! ❌ No second content reader: the read goes through `inspect_path`, whose sensitive
//! gate and private-key prescan run first, and this module only cuts what came back
//! down to what the consent copy promises: about 40 lines of text, a PDF's title and
//! first page, an archive's first 20 names, a photo's camera details without its
//! location. ❌ GPS never leaves: [`Peek::CameraDetails`] has no field for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::super::event::VerdictContents;
use crate::security_log::ContentTool;
use crate::tools::read::inspect::{Content, FileRow, TextAsk, WindowOpts};
use crate::tools::sensitive;

/// Lines of a text file the look reads.
pub(crate) const PEEK_LINES: usize = 40;
/// The most characters of text (a window, or a PDF page) the look keeps.
pub(crate) const PEEK_CHARS: usize = 2_000;
/// The most archive entry names the look keeps.
pub(crate) const PEEK_ENTRIES: usize = 20;

/// Reads one path the way `inspect_file` does: injectable, so a test can count reads.
pub(crate) type ReadFn = Arc<dyn Fn(&str, &TextAsk, &AtomicBool) -> FileRow + Send + Sync>;

/// The real reader: `inspect_file`'s core, gate first.
pub(crate) fn inspect_reader() -> ReadFn {
    Arc::new(crate::tools::read::inspect::inspect_path)
}

/// What the look found, as the model reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum Peek {
    /// The first lines of a text file.
    FirstLines {
        text: String,
        /// More of the file follows.
        more: bool,
    },
    /// A PDF's title and its first page's text.
    FirstPage {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        page_count: Option<usize>,
        text: String,
    },
    /// An archive's first entries.
    EntryNames {
        format: String,
        names: Vec<String>,
        /// How many entries the archive's top level holds in all.
        total: usize,
    },
    /// What the camera wrote, ❌ never where.
    CameraDetails {
        #[serde(skip_serializing_if = "Option::is_none")]
        date_taken: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        camera_make: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        camera_model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        lens: Option<String>,
    },
}

/// The look's window: the first [`PEEK_LINES`] lines, a PDF's first page.
fn ask() -> TextAsk {
    TextAsk::Window(WindowOpts {
        start_line: 1,
        max_lines: PEEK_LINES,
        page_start: 1,
        max_pages: 1,
    })
}

/// Looks inside `path` once, on a thread of its own, waiting at most `budget`. Past it
/// the read's cancel flag flips and the verdict goes out with the facts alone.
pub(crate) async fn look(path: &str, read: ReadFn, budget: Duration) -> (VerdictContents, Option<Peek>) {
    let cancel = Arc::new(AtomicBool::new(false));
    let (answer, waiting) = tokio::sync::oneshot::channel();
    let owned = path.to_string();
    let flag = Arc::clone(&cancel);
    // Its own thread, ❌ never the about pool: queued behind the card's own slow jobs it
    // would miss its budget on a healthy disk. One verdict runs at a time, so a read
    // stranded on a disk that stopped answering is one thread.
    let spawned = std::thread::Builder::new()
        .name("about-verdict-peek".to_string())
        .spawn(move || {
            let _ = answer.send(read(&owned, &ask(), &flag));
        });
    if spawned.is_err() {
        log::warn!(target: "about", "the verdict's look inside couldn't start a thread; facts only");
        return (VerdictContents::None, None);
    }
    match tokio::time::timeout(budget, waiting).await {
        Ok(Ok(row)) => shape(row),
        Ok(Err(_)) => (VerdictContents::None, None),
        Err(_) => {
            cancel.store(true, Ordering::Relaxed);
            log::debug!(target: "about", "the verdict's look inside ran past its budget; facts only");
            (VerdictContents::None, None)
        }
    }
}

/// What a read answered, cut down to the look. A protected file is logged by tool and
/// reason, ❌ never by path.
pub(crate) fn shape(row: FileRow) -> (VerdictContents, Option<Peek>) {
    let file = match row {
        FileRow::Ok(file) => file,
        FileRow::Protected { reason, .. } => {
            sensitive::note_refusal(ContentTool::AboutVerdict, reason);
            return (VerdictContents::Protected, None);
        }
        FileRow::Folder { .. }
        | FileRow::Missing { .. }
        | FileRow::NoVolumeAtPath { .. }
        | FileRow::Unreadable { .. }
        | FileRow::Unreachable { .. }
        | FileRow::UnsupportedVolume { .. } => return (VerdictContents::None, None),
    };
    match file.content {
        Content::Text(text) => match text.window {
            Some(window) if !window.content.trim().is_empty() => {
                let (text, cut) = cap(&window.content);
                (
                    VerdictContents::FirstLines,
                    Some(Peek::FirstLines {
                        text,
                        more: cut || window.truncated,
                    }),
                )
            }
            _ => (VerdictContents::None, None),
        },
        Content::Pdf(pdf) => {
            let first = pdf.pages.as_ref().and_then(|window| window.pages.first());
            let page = first.map(|page| cap(&page.text).0).unwrap_or_default();
            let title = pdf.title.as_deref().map(|title| cap(title).0);
            if page.is_empty() && title.is_none() {
                return (VerdictContents::None, None);
            }
            (
                VerdictContents::FirstPage,
                Some(Peek::FirstPage {
                    title,
                    page_count: pdf.page_count,
                    text: page,
                }),
            )
        }
        Content::Archive(archive) => {
            if archive.entries.is_empty() {
                return (VerdictContents::None, None);
            }
            let names = archive
                .entries
                .iter()
                .take(PEEK_ENTRIES)
                .map(|entry| cmdr_about::text::model_text(&entry.name))
                .collect();
            (
                VerdictContents::EntryNames,
                Some(Peek::EntryNames {
                    format: archive.format,
                    names,
                    total: archive.total,
                }),
            )
        }
        Content::Image(image) => match image.exif {
            // ❗ The GPS field is read past, by construction: `CameraDetails` can't hold it.
            Some(exif) if exif.camera_make.is_some() || exif.camera_model.is_some() || exif.date_taken.is_some() => (
                VerdictContents::CameraDetails,
                Some(Peek::CameraDetails {
                    date_taken: exif.date_taken.as_deref().map(cmdr_about::text::model_text),
                    camera_make: exif.camera_make.as_deref().map(cmdr_about::text::model_text),
                    camera_model: exif.camera_model.as_deref().map(cmdr_about::text::model_text),
                    lens: exif.lens.as_deref().map(cmdr_about::text::model_text),
                }),
            ),
            _ => (VerdictContents::None, None),
        },
        Content::Empty {} | Content::Binary {} => (VerdictContents::None, None),
    }
}

/// `text` with control and direction-override characters dropped (newlines and tabs
/// kept: they're what makes lines lines), cut at [`PEEK_CHARS`]. Answers whether it cut.
fn cap(text: &str) -> (String, bool) {
    let mut kept = text.chars().filter(|c| matches!(c, '\n' | '\t') || !unsafe_char(*c));
    let out: String = kept.by_ref().take(PEEK_CHARS).collect();
    (out, kept.next().is_some())
}

/// A control character, or a mark that makes text read in another direction than it's
/// stored: the same set `cmdr_about::text::model_text` drops.
fn unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        )
}

#[cfg(test)]
#[path = "peek_tests.rs"]
mod tests;
