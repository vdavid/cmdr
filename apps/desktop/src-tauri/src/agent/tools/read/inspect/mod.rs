//! The `inspect_file` agent tool: "what's in this file?", answered from the file itself,
//! for up to [`MAX_PATHS`] files in one call.
//!
//! Each row carries metadata (size, modified, extension, MIME guess), the kind the BYTES
//! say the file is (the viewer's classifier, never the extension alone), and a typed
//! `content` per kind:
//!
//! - **text**: a line window (`startLine` + `maxLines`, capped in chars) read through the
//!   viewer's own line backends and encoding detection, so a UTF-16 or Windows-1252 file
//!   reads as text, exactly as the viewer shows it. Every cut is named. With `find`, the
//!   matching lines take the window's place (`find.rs`), found with the viewer's search.
//! - **image**: format and dimensions from the header, and a pointer to `image_facts` for
//!   what's IN the picture. Image BYTES never cross (the DTO is text-only).
//! - **archive**: the immediate children of a `.zip` / tar / 7z, or of a directory inside
//!   one, listed through the same volume routing the pane browses with; a FILE inside an
//!   archive is extracted to the viewer's bounded temp and read as its own kind
//!   (`archive.rs`).
//! - **empty** / **binary**: metadata only. PDFs are `binary` here: their text needs a
//!   parser this tool doesn't carry.
//!
//! Every path gets a typed status (`ok` / `folder` / `missing` / `unreadable` /
//! `unreachable` / `unsupportedVolume`), the whole answer is cut to the tool-result
//! budget, and `unanswered` names each path that got no row, so the model can ask again
//! for the rest. Reuse map and the seams this rides: `../../DETAILS.md` § Reading a file
//! the way the viewer does.
//!
//! ## Privacy
//!
//! The one tool that hands a provider the CONTENTS of arbitrary user files (bounded, on
//! request). It rides the Ask Cmdr consent gate; the consent copy has to name it before
//! this ships to users.
//!
//! ## Never hangs the turn
//!
//! Each path reads on a blocking thread under [`PATH_TIMEOUT`], four at a time, inside
//! [`CALL_TIMEOUT`] for the call. A dead mount answers `unreachable`, never a wedged turn
//! (principle 2, Rock solid). What the tool does with the thread it can't stop: `runner`.

mod archive;
#[cfg(test)]
mod archive_tests;
mod find;
#[cfg(test)]
mod find_tests;
mod runner;
#[cfg(test)]
mod tests;
mod text;

use std::collections::HashSet;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_viewer::archive_extract::extract_if_archive_inner;
use crate::file_viewer::content_kind::{
    CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, looks_binary, media_mime,
};
use crate::file_viewer::encoding::detect_from_head;
use crate::file_viewer::media::read_image_dimensions;
use crate::file_viewer::{Matcher, SearchMode, ViewerError};
use crate::mcp::{ToolError, ToolResult, fit_to_result_budget, is_virtual_path};
use crate::search::{format_size, format_timestamp};

pub use archive::{ArchiveContent, ArchiveEntry};
use archive::{ExtractFn, Routed};
pub use find::{FindHits, FindLine};
use runner::{InspectFn, RunnerConfig};
pub use text::TextWindow;

/// The most paths one call accepts; over it is `INVALID_PARAMS`, never a silent cut
/// (mirrors `image_facts::MAX_PATHS`). The answer's SIZE is bounded separately, by
/// `fit_to_result_budget`.
pub(crate) const MAX_PATHS: usize = 200;
/// Lines in the window when the caller doesn't say.
const DEFAULT_MAX_LINES: usize = 200;
/// The most lines one window holds; a larger ask is clamped (the window reports what it
/// returned, so the clamp is visible).
const MAX_MAX_LINES: usize = 2_000;
/// The most text one row returns, in chars, whatever the line count. The outer
/// `fit_to_result_budget` still decides how many rows fit.
pub(crate) const MAX_WINDOW_CHARS: usize = 16_000;
/// A single line longer than this is cut and the row says so (`linesCut`). A minified
/// bundle is one line of 4 MB; the model needs to know it IS one line, not read it.
pub(crate) const MAX_LINE_CHARS: usize = 2_000;
/// Bytes read once from the head: 64 KB is what `encoding::detect_from_head` wants, and
/// the classifier takes the first [`CLASSIFY_HEAD_LEN`] of the same buffer.
const HEAD_LEN: usize = 64 * 1024;
/// A path's whole budget. Past it the row is `unreachable`: a disconnected drive or a
/// hung mount, not a slow disk.
const PATH_TIMEOUT: Duration = Duration::from_secs(5);
/// Of that budget, how long after the cancel flag flips a path may still hand back a
/// partial, flagged window (the ByteSeek fallback opens in milliseconds on a live disk).
const CANCEL_GRACE: Duration = Duration::from_secs(1);
/// The whole call. Past it no new path starts and the rest lands in `unanswered`.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
/// Paths read at once: enough to overlap disk latency, few enough that a dead mount
/// parks at most this many blocking threads.
const PATH_CONCURRENCY: usize = 4;
/// Where a picture's contents live.
const IMAGE_HINT: &str = "For recognized text and tags inside the picture, call image_facts with this path.";

// ── Result DTOs ─────────────────────────────────────────────────────────────

/// Why a file that exists couldn't be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnreadableReason {
    /// EACCES, or a Full Disk Access refusal: the two aren't distinguishable from
    /// `std::io::Error`, so the enum doesn't pretend to.
    Permission,
    /// Any other I/O failure (a failing disk, a symlink loop, a read that panicked).
    Io,
    /// A password-protected archive, or an encrypted entry inside one. The tool has no
    /// password path (the viewer prompts the user; the agent can't).
    Encrypted,
    /// An archive whose parse found a damaged structure (the archive layer's `IoError`).
    Corrupt,
    /// An archive the archive layer can't or won't serve: an unsupported codec, a file
    /// that isn't the archive its name claims, or a tree past its DoS cap (its
    /// `NotSupported`). Not a damaged file, so never reported as one.
    Unsupported,
    /// A file inside an archive over the viewer's 256 MiB extraction cap
    /// (`archive_extract::EXTRACT_CAP_BYTES`), refused before any byte was extracted.
    TooLargeToExtract,
}

/// What the header of an image says. Never pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    /// `content_kind::media_mime` from the magic bytes: `image/jpeg`, `image/heic`, ...
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub hint: &'static str,
}

/// A text file: its encoding, how many lines it has when that's known, and either one
/// window or the lines matching `find`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    /// `FileEncoding::label()`: "UTF-8", "UTF-16 LE", "Western (Windows-1252)", ...
    pub encoding: String,
    /// Known for a fully loaded file or a finished line index; absent on the ByteSeek
    /// fallback and on a `find` over a large file (a scan builds no index). Counts the
    /// trailing empty line after a final newline, as the viewer's line numbers do, so
    /// "line 812" here is line 812 in the viewer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<usize>,
    /// `true` only when the line index didn't finish in time and the window was read by
    /// byte estimate: `startLine` and `totalLines` are then not to be quoted as exact.
    /// Never set on a `find` row: the scan numbers lines exactly on every backend.
    #[serde(skip_serializing_if = "super::is_false")]
    pub line_numbers_approximate: bool,
    /// The line window. Absent when `find` is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<TextWindow>,
    /// The matching lines. Present exactly when the call carried `find`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub find: Option<FindHits>,
}

/// The per-kind content section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Content {
    Empty {},
    Binary {},
    Text(TextContent),
    Image(ImageContent),
    Archive(ArchiveContent),
}

/// A file that was read: its metadata and per-kind content. Boxed inside [`FileRow::Ok`]
/// so the enum stays the size of its small status variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectedFile {
    /// The path exactly as requested (after `~` expansion), so the caller can join back.
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// Absent only for a directory inside an archive, which has no size to report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>,
    /// Last modified, RFC 3339 UTC seconds. Absent when the filesystem doesn't say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// The same instant as `YYYY-MM-DD`, so the model never does date arithmetic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_human: Option<String>,
    /// MIME type guessed from the extension. Beside `content.kind` / `format` so a lying
    /// extension shows. Absent when the extension is unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    pub content: Content,
}

/// One requested path's answer: a typed status the model relays honestly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum FileRow {
    /// The file was read; its fields sit beside `status` (serde flattens the boxed struct).
    Ok(Box<InspectedFile>),
    /// The path is a folder: use `list_dir` for its children.
    Folder { path: String },
    /// Nothing at that path.
    Missing { path: String },
    /// The file exists but Cmdr can't read it.
    Unreadable { path: String, reason: UnreadableReason },
    /// [`PATH_TIMEOUT`] passed: a disconnected drive or a hung mount.
    Unreachable { path: String },
    /// An `mtp://` or direct `smb://` path, or a path on a registered volume without
    /// local filesystem access: no local byte path to read (the viewer has the same
    /// limit).
    UnsupportedVolume { path: String },
}

impl FileRow {
    /// The requested path this row answers for.
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Ok(file) => &file.path,
            Self::Folder { path }
            | Self::Missing { path }
            | Self::Unreadable { path, .. }
            | Self::Unreachable { path }
            | Self::UnsupportedVolume { path } => path,
        }
    }
}

/// The call result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum InspectResult {
    Ok {
        /// One row per answered path, in request order, possibly with gaps (see `unanswered`).
        files: Vec<FileRow>,
        /// Paths asked about.
        total: usize,
        /// Rows carried.
        returned: usize,
        /// `returned < total`: rows were cut by the size ceiling, or paths never finished
        /// inside the call deadline. Ask again for `unanswered`.
        truncated: bool,
        /// Every requested path with no row in this answer.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        unanswered: Vec<String>,
    },
}

// ── Params ────────────────────────────────────────────────────────────────────

pub fn inspect_file_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Absolute file paths (~ ok), at most 200; check returned, total, truncated, and unanswered."
            },
            "startLine": {
                "type": "integer", "minimum": 1,
                "description": "Text: first window line (1-based, default 1)."
            },
            "maxLines": {
                "type": "integer", "minimum": 1, "maximum": MAX_MAX_LINES,
                "description": "Text: window lines (default 200)."
            },
            "find": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "regex": { "type": "boolean" },
                    "caseSensitive": { "type": "boolean" }
                },
                "required": ["query"],
                "description": "Search text files; matching lines replace the window."
            }
        },
        "required": ["paths"],
        "additionalProperties": false
    })
}

/// Which lines a text row's window covers. Copied into every path's read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowOpts {
    /// 1-based.
    pub start_line: usize,
    pub max_lines: usize,
}

/// What every text row in the call carries: its window, or the lines matching `find`.
#[derive(Debug, Clone)]
pub(crate) enum TextAsk {
    Window(WindowOpts),
    /// One matcher for the whole call, shared by every path, as the viewer builds one per
    /// search: the regex compiles once.
    Find(Arc<Matcher>),
}

#[derive(Debug)]
struct Params {
    paths: Vec<String>,
    ask: TextAsk,
}

/// `find`, when present, as one built matcher. A query the viewer's matcher rejects (an
/// invalid regex, a pattern that would have to cross a newline) is `INVALID_PARAMS` with
/// the matcher's own reason, before any file is opened.
fn parse_find(params: &Value) -> Result<Option<Arc<Matcher>>, ToolError> {
    let find = match params.get("find") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v
            .as_object()
            .ok_or_else(|| ToolError::invalid_params("'find' must be an object with a 'query' string"))?,
    };
    let query = find
        .get("query")
        .and_then(Value::as_str)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| ToolError::invalid_params("'find.query' must be a non-empty string"))?;
    let flag = |name: &str| match find.get(name) {
        None | Some(Value::Null) => Ok(false),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| ToolError::invalid_params(format!("'find.{name}' must be true or false"))),
    };
    let mode = SearchMode {
        use_regex: flag("regex")?,
        case_sensitive: flag("caseSensitive")?,
    };
    let matcher = Matcher::build(query, mode).map_err(|e| ToolError::invalid_params(format!("'find.query': {e}")))?;
    Ok(Some(Arc::new(matcher)))
}

fn parse_params(params: &Value) -> Result<Params, ToolError> {
    let raw = params
        .get("paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ToolError::invalid_params("Missing 'paths' parameter (an array of absolute file paths)"))?;
    if raw.is_empty() {
        return Err(ToolError::invalid_params("'paths' must list at least one file"));
    }
    if raw.len() > MAX_PATHS {
        return Err(ToolError::invalid_params(format!(
            "'paths' holds {} files, more than the {MAX_PATHS} this answers at once. Ask about them in batches.",
            raw.len()
        )));
    }
    let paths = raw
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(super::expand_tilde)
                .ok_or_else(|| ToolError::invalid_params("Every entry in 'paths' must be a non-empty file path"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let start_line =
        match params.get("startLine") {
            None | Some(Value::Null) => 1,
            Some(v) => v.as_u64().filter(|n| *n >= 1).map(|n| n as usize).ok_or_else(|| {
                ToolError::invalid_params("'startLine' must be a positive integer (lines are 1-based)")
            })?,
        };
    let max_lines = match params.get("maxLines") {
        None | Some(Value::Null) => DEFAULT_MAX_LINES,
        Some(v) => v
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|n| (n as usize).min(MAX_MAX_LINES))
            .ok_or_else(|| ToolError::invalid_params("'maxLines' must be a positive integer"))?,
    };
    let ask = match parse_find(params)? {
        Some(matcher) => TextAsk::Find(matcher),
        None => TextAsk::Window(WindowOpts { start_line, max_lines }),
    };
    Ok(Params { paths, ask })
}

// ── One path, on a blocking thread ────────────────────────────────────────────

/// Why one path's read stopped short of a row.
enum ReadFailure {
    Io(std::io::Error),
    Viewer(ViewerError),
}

impl From<std::io::Error> for ReadFailure {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ViewerError> for ReadFailure {
    fn from(e: ViewerError) -> Self {
        Self::Viewer(e)
    }
}

/// Inspect one path. Blocking: runs on the pool under the runner's deadline, which flips
/// `cancel` when the path is out of time (the line-index build and the search check it).
pub(crate) fn inspect_path(path: &str, ask: &TextAsk, cancel: &AtomicBool) -> FileRow {
    inspect_path_with(path, ask, cancel, &extract_if_archive_inner)
}

/// [`inspect_path`] with the archive extract step injected (tests shrink its cap and
/// redirect its temp).
pub(crate) fn inspect_path_with(path: &str, ask: &TextAsk, cancel: &AtomicBool, extract: ExtractFn) -> FileRow {
    let owned = path.to_string();
    if is_virtual_path(path) {
        return FileRow::UnsupportedVolume { path: owned };
    }
    // The volume that holds the path: the longest registered mount root over it, else the
    // main drive. It is the archive's parent for routing, and the one place a path with no
    // scheme can still be unreadable through `std::fs` (a device volume's namespace).
    let manager = get_volume_manager();
    let volume_id = manager.mount_id_for_path(path).unwrap_or_else(|| "root".to_string());
    if manager
        .get(&volume_id)
        .is_some_and(|volume| !volume.supports_local_fs_access())
    {
        return FileRow::UnsupportedVolume { path: owned };
    }
    if let Routed::Row(row) = archive::route(path, &volume_id, ask, cancel, extract) {
        return row;
    }

    let p = Path::new(path);
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) => return status_for(owned, ReadFailure::Io(e)),
    };
    if meta.is_dir() {
        return FileRow::Folder { path: owned };
    }
    let size = meta.len();
    match read_content(p, size, ask, cancel) {
        Ok(content) => ok_row(owned, p, Some(size), modified_secs(&meta), content),
        Err(failure) => status_for(owned, failure),
    }
}

/// An `ok` row: the metadata every kind shares, named after the requested path.
fn ok_row(path: String, p: &Path, size: Option<u64>, modified_secs: Option<u64>, content: Content) -> FileRow {
    let (modified, modified_human) = spoken_modified(modified_secs);
    FileRow::Ok(Box::new(InspectedFile {
        path,
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        extension: p.extension().map(|e| e.to_string_lossy().to_lowercase()),
        size_bytes: size,
        size_human: size.map(format_size),
        modified,
        modified_human,
        mime: mime_guess::from_path(p).first().map(|m| m.essence_str().to_string()),
        content,
    }))
}

/// A stat's mtime as Unix seconds, when the filesystem says.
fn modified_secs(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// A timestamp as the model reads it: RFC 3339 UTC seconds, and its `YYYY-MM-DD` twin
/// (`search::format_timestamp`, never a second formatter).
fn spoken_modified(secs: Option<u64>) -> (Option<String>, Option<String>) {
    (
        secs.and_then(|secs| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(secs as i64, 0)
                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        }),
        secs.map(format_timestamp),
    )
}

/// The typed status for a read that stopped short.
fn status_for(path: String, failure: ReadFailure) -> FileRow {
    match failure {
        ReadFailure::Io(e) => match e.kind() {
            ErrorKind::NotFound | ErrorKind::NotADirectory => FileRow::Missing { path },
            ErrorKind::PermissionDenied => FileRow::Unreadable {
                path,
                reason: UnreadableReason::Permission,
            },
            _ => FileRow::Unreadable {
                path,
                reason: UnreadableReason::Io,
            },
        },
        ReadFailure::Viewer(ViewerError::NotFound { .. }) => FileRow::Missing { path },
        ReadFailure::Viewer(ViewerError::IsDirectory) => FileRow::Folder { path },
        ReadFailure::Viewer(ViewerError::ExtractTooLarge { .. }) => FileRow::Unreadable {
            path,
            reason: UnreadableReason::TooLargeToExtract,
        },
        // The archive layer's typed "couldn't serve this entry": damaged, or a codec it
        // doesn't decode. (An encrypted entry never gets here; `archive.rs` refuses it
        // from the index before extracting.)
        ReadFailure::Viewer(ViewerError::Archive { .. }) => FileRow::Unreadable {
            path,
            reason: UnreadableReason::Corrupt,
        },
        ReadFailure::Viewer(_) => FileRow::Unreadable {
            path,
            reason: UnreadableReason::Io,
        },
    }
}

/// Read the head once, classify with the viewer's classifier, and shape the content for
/// the kind. `ext = None` keeps SVG on the text path (its markup says more to a model
/// than "an image"); `is_local = true` because the row is already a local file.
fn read_content(p: &Path, size: u64, ask: &TextAsk, cancel: &AtomicBool) -> Result<Content, ReadFailure> {
    if size == 0 {
        return Ok(Content::Empty {});
    }
    let mut head = Vec::with_capacity(HEAD_LEN.min(size as usize));
    std::fs::File::open(p)?.take(HEAD_LEN as u64).read_to_end(&mut head)?;
    let classify_head = &head[..head.len().min(CLASSIFY_HEAD_LEN)];
    match classify_viewer_content(classify_head, None, true) {
        ViewerContentKind::Image => Ok(Content::Image(image_content(p, classify_head))),
        // A PDF's text needs a parser this tool doesn't carry; `binary` is the honest
        // kind rather than a section that says "not wired".
        ViewerContentKind::Pdf => Ok(Content::Binary {}),
        ViewerContentKind::Text => {
            let encoding = detect_from_head(&head);
            if looks_binary(&head, encoding) {
                return Ok(Content::Binary {});
            }
            Ok(Content::Text(text::read_text(p, encoding, ask, cancel)?))
        }
    }
}

fn image_content(p: &Path, head: &[u8]) -> ImageContent {
    let dims = read_image_dimensions(p);
    ImageContent {
        format: media_mime(head, ViewerContentKind::Image)
            .unwrap_or("application/octet-stream")
            .to_string(),
        width: dims.map(|d| d.0),
        height: dims.map(|d| d.1),
        hint: IMAGE_HINT,
    }
}

// ── The call ──────────────────────────────────────────────────────────────────

/// The production timeout policy: the constants above, as the runner takes them.
const RUNNER: RunnerConfig = RunnerConfig {
    path_timeout: PATH_TIMEOUT,
    cancel_grace: CANCEL_GRACE,
    call_timeout: CALL_TIMEOUT,
    concurrency: PATH_CONCURRENCY,
};

/// Build the answer from the runner's slots: the rows that exist, cut to what one tool
/// result may carry, plus `unanswered` for every requested path that has no row in the
/// kept prefix (never launched, abandoned without a row, or cut by the ceiling). Pure.
pub(crate) fn shape_ok(paths: &[String], rows: Vec<Option<FileRow>>) -> InspectResult {
    let fitted = fit_to_result_budget(rows.into_iter().flatten().collect());
    let kept: HashSet<&str> = fitted.items.iter().map(FileRow::path).collect();
    let unanswered: Vec<String> = paths.iter().filter(|p| !kept.contains(p.as_str())).cloned().collect();
    InspectResult::Ok {
        total: paths.len(),
        returned: fitted.items.len(),
        truncated: fitted.items.len() < paths.len(),
        unanswered,
        files: fitted.items,
    }
}

/// The call, minus the wire: parse, inspect every path on the blocking pool (bounded per
/// path and per call), shape.
pub(crate) async fn run(params: &Value) -> Result<InspectResult, ToolError> {
    let Params { paths, ask } = parse_params(params)?;
    let inspect: InspectFn = Arc::new(move |path, cancel| inspect_path(path, &ask, cancel));
    let rows = runner::run_paths(&paths, &RUNNER, inspect).await;
    Ok(shape_ok(&paths, rows))
}

/// Handler.
pub async fn execute_inspect_file<R: Runtime>(_app: &AppHandle<R>, params: &Value) -> ToolResult {
    serde_json::to_value(run(params).await?).map_err(|e| ToolError::internal(e.to_string()))
}
