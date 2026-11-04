//! The file viewer's commands, answered by the real viewer core (`cmdr-viewer`) over the
//! same in-memory volume the listing reads, the way the desktop's Tauri commands answer
//! them (`apps/desktop/src-tauri/src/commands/file_viewer.rs`): the same names, the same
//! camelCase arguments, the same JSON back, including which errors are typed and which
//! are the plain strings the app sends.
//!
//! The app's commands run under 2 s timeouts and stall watches on blocking threads; here
//! every call runs inline on the page's one thread and the volume answers at once
//! (`DETAILS.md` § "Where the demo differs from the app").

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cmdr_fs::rt::Runtime;
use cmdr_fs::volume::Volume;
use cmdr_viewer::host::ViewerHost;
use cmdr_viewer::host::events::{FileChange, ViewerEvents};
use cmdr_viewer::host::images::InPlace;
use cmdr_viewer::host::opener::LinkOpener;
use cmdr_viewer::host::volumes::ViewerVolumes;
use cmdr_viewer::media::MediaSource;
use cmdr_viewer::media_serve::{MediaAnswer, serve_volume_inline};
use cmdr_viewer::{
    FileEncoding, MediaLoad, RangeEnd, RowText, SearchMode, SeekTarget, SeekTargetKind, Viewer, ViewerError,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::EventSink;
use crate::invoke::{answer, ok, parse};

/// Every viewer command [`DemoViewer::invoke`] answers, by its Tauri name.
pub const VIEWER_COMMANDS: &[&str] = &[
    "viewer_open",
    "viewer_open_as_text",
    "viewer_get_lines",
    "viewer_get_bytes",
    "viewer_render_markdown",
    "viewer_allow_remote_images",
    "viewer_follow_markdown_link",
    "viewer_search_start",
    "viewer_search_poll",
    "viewer_search_cancel",
    "viewer_get_status",
    "viewer_close",
    "viewer_read_range",
    "viewer_cancel_read",
    "viewer_write_range_to_file",
    "viewer_get_encoding_options",
    "viewer_set_encoding",
    "viewer_set_tail_mode",
    "viewer_reload",
];

/// The demo's event for a media token that dropped, so the page revokes the `blob:` URL
/// it made for it. Not a Tauri event: the page's backend consumes it.
pub const MEDIA_TOKEN_DROPPED: &str = "demo://media-token-dropped";
/// The demo's event for a web or mail link the person followed, so the page opens it.
pub const OPEN_URL: &str = "demo://open-url";
/// The desktop's media progress event (`ViewerMediaLoad` in the app), sent to one window.
pub const VIEWER_MEDIA_LOAD: &str = "viewer-media-load";

/// One viewer over the demo's volume.
#[derive(Clone)]
pub struct DemoViewer {
    viewer: Viewer,
    sink: Arc<dyn EventSink>,
}

/// One media token's bytes, as the page turns them into a `blob:` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaBody {
    /// The `Content-Type`, from the file's magic bytes.
    pub mime: String,
    /// The whole file.
    pub bytes: Vec<u8>,
}

impl DemoViewer {
    /// A viewer reading `volume` (served under `volume_id`), reporting to `sink`, bridging
    /// on `runtime` (the page's event loop in the browser).
    pub fn new(volume_id: String, volume: Arc<dyn Volume>, sink: Arc<dyn EventSink>, runtime: Runtime) -> Self {
        let volumes: Arc<dyn ViewerVolumes> = Arc::new(OneVolume { volume_id, volume });
        let host = ViewerHost {
            volumes: Arc::clone(&volumes),
            images: Arc::new(InPlace::new(volumes, runtime.clone())),
            events: Arc::new(PageEvents(Arc::clone(&sink))),
            opener: Arc::new(PageOpener(Arc::clone(&sink))),
            // The page has no disk: every file is the volume's.
            local_files: false,
            ..ViewerHost::detached()
        };
        Self {
            viewer: Viewer::new(host, runtime),
            sink,
        }
    }

    /// Whether [`DemoViewer::invoke`] answers `command`.
    pub fn handles(command: &str) -> bool {
        VIEWER_COMMANDS.contains(&command)
    }

    /// Runs a viewer command with Tauri-style `args` (camelCase keys).
    pub fn invoke(&self, command: &str, args: Value) -> Result<Value, Value> {
        let v = &self.viewer;
        match command {
            "viewer_open" => {
                let a: OpenArgs = parse(command, args)?;
                answer(self.open(&a.path, &a.volume_id, &a.window_label, false, a.rows))
            }
            "viewer_open_as_text" => {
                let a: OpenAsTextArgs = parse(command, args)?;
                answer(self.open(&a.path, &a.volume_id, &a.window_label, true, RowText::Raw))
            }
            "viewer_get_lines" => {
                let a: LinesArgs = parse(command, args)?;
                let target = match a.target_type {
                    SeekTargetKind::Row => SeekTarget::Row(a.target_value as usize),
                    SeekTargetKind::Byte => SeekTarget::ByteOffset(a.target_value as u64),
                    SeekTargetKind::Fraction => SeekTarget::Fraction(a.target_value),
                };
                answer(v.get_lines(&a.session_id, target, a.count, &AtomicBool::new(false)))
            }
            "viewer_get_bytes" => {
                let a: BytesArgs = parse(command, args)?;
                answer(v.get_bytes(&a.session_id, a.offset, a.count))
            }
            "viewer_render_markdown" => {
                let a: SessionArgs = parse(command, args)?;
                answer(v.render_markdown_session(&a.session_id))
            }
            // Allowed as in the app; the page reaches no network, so every web picture
            // answers 404 (`InPlace`).
            "viewer_allow_remote_images" => {
                let a: SessionArgs = parse(command, args)?;
                answer(v.allow_remote_images(&a.session_id))
            }
            "viewer_follow_markdown_link" => {
                let a: LinkArgs = parse(command, args)?;
                answer(v.follow_markdown_link(&a.session_id, &a.href))
            }
            "viewer_search_start" => {
                let a: SearchArgs = parse(command, args)?;
                if a.query.is_empty() {
                    return Err(Value::String("Search query cannot be empty".to_string()));
                }
                as_string(v.search_start(&a.session_id, a.query, a.mode))
            }
            "viewer_search_poll" => {
                let a: PollArgs = parse(command, args)?;
                as_string(v.search_poll(&a.session_id, a.since_index))
            }
            "viewer_search_cancel" => {
                let a: SessionArgs = parse(command, args)?;
                as_string(v.search_cancel(&a.session_id))
            }
            "viewer_get_status" => {
                let a: SessionArgs = parse(command, args)?;
                as_string(v.get_session_status(&a.session_id))
            }
            "viewer_close" => {
                let a: SessionArgs = parse(command, args)?;
                as_string(v.close_session(&a.session_id))
            }
            "viewer_read_range" => {
                let a: RangeArgs = parse(command, args)?;
                answer(v.read_range(&a.session_id, a.read_id, a.anchor, a.focus))
            }
            "viewer_cancel_read" => {
                let a: CancelReadArgs = parse(command, args)?;
                answer(v.cancel_read(&a.session_id, a.read_id))
            }
            // The demo saves nothing, and says so: its volume is read-only.
            "viewer_write_range_to_file" => answer::<(), _>(Err(ViewerError::DestinationIsReadOnly)),
            "viewer_get_encoding_options" => {
                let a: SessionArgs = parse(command, args)?;
                as_string(v.get_encoding_options(&a.session_id))
            }
            "viewer_set_encoding" => {
                let a: EncodingArgs = parse(command, args)?;
                answer(v.set_encoding(&a.session_id, a.encoding))
            }
            // Accepted as in the app; nothing grows a demo file while it's open, since the
            // browser has no watcher.
            "viewer_set_tail_mode" => {
                let a: TailArgs = parse(command, args)?;
                answer(v.set_tail_mode(&a.session_id, a.enabled))
            }
            "viewer_reload" => {
                let a: SessionArgs = parse(command, args)?;
                answer(v.reload(&a.session_id))
            }
            _ => Err(Value::String(format!("the demo's viewer doesn't answer `{command}`"))),
        }
    }

    /// One open, registered as the window's pending open for its run, as the app's is.
    fn open(
        &self,
        path: &str,
        volume_id: &str,
        window_label: &str,
        force_text: bool,
        rows: RowText,
    ) -> Result<cmdr_viewer::ViewerOpenResult, ViewerError> {
        let pending = self.viewer.begin_pending_open(window_label);
        let result = self
            .viewer
            .open_for_window(path, volume_id, window_label, force_text, rows, &pending);
        self.viewer.end_pending_open(window_label, &pending);
        result
    }

    /// Frees every session the viewer window `window_label` holds: the app's answer to
    /// that window's `Destroyed`.
    pub fn close_window(&self, window_label: &str) {
        self.viewer.close_session_for_window(window_label);
    }

    /// The bytes behind a media token, for the viewer window `window_label`, which hears
    /// `viewer-media-load` `Done` as the app's window does. `None` for an unknown or
    /// dropped token, or bytes that can't be had (the app's 404).
    pub fn media_body(&self, token: &str, window_label: &str) -> Option<MediaBody> {
        let MediaSource::Volume(media) = self.viewer.tokens().resolve_token(token)? else {
            // Every demo session reads through its volume, so nothing mints the other kinds.
            return None;
        };
        let emit = |load: MediaLoad| match serde_json::to_value(load) {
            Ok(payload) => self.sink.emit_to(window_label, VIEWER_MEDIA_LOAD, payload),
            Err(e) => log::warn!(target: "viewer", "a media load didn't serialize: {e}"),
        };
        match serve_volume_inline(&media, token, None, emit) {
            MediaAnswer::Body { mime, body, .. } => Some(MediaBody { mime, bytes: body }),
            MediaAnswer::Unsatisfiable { .. } | MediaAnswer::NotFound | MediaAnswer::TimedOut => None,
        }
    }
}

/// The app sends these commands' errors as the error's text (`map_err(|e| e.to_string())`).
fn as_string<T: serde::Serialize>(result: Result<T, ViewerError>) -> Result<Value, Value> {
    match result {
        Ok(answer) => ok(answer),
        Err(error) => Err(Value::String(error.to_string())),
    }
}

/// The demo's one volume, by its id.
struct OneVolume {
    volume_id: String,
    volume: Arc<dyn Volume>,
}

impl ViewerVolumes for OneVolume {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        (volume_id == self.volume_id).then(|| Arc::clone(&self.volume))
    }

    fn routes_over_its_parent(&self, _path: &Path) -> bool {
        false
    }
}

/// Viewer facts as page events.
struct PageEvents(Arc<dyn EventSink>);

impl ViewerEvents for PageEvents {
    /// Never raised: a browser session has no watcher.
    fn file_changed(&self, _session_id: &str, _change: FileChange) {}

    fn media_token_dropped(&self, token: &str) {
        self.0.emit(MEDIA_TOKEN_DROPPED, json!({ "token": token }));
    }
}

/// A followed web or mail link, handed to the page to open.
struct PageOpener(Arc<dyn EventSink>);

impl LinkOpener for PageOpener {
    fn open_url(&self, url: &str) -> Result<(), ViewerError> {
        self.0.emit(OPEN_URL, json!({ "url": url }));
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenArgs {
    path: String,
    volume_id: String,
    window_label: String,
    rows: RowText,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenAsTextArgs {
    path: String,
    volume_id: String,
    window_label: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionArgs {
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinesArgs {
    session_id: String,
    target_type: SeekTargetKind,
    target_value: f64,
    count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BytesArgs {
    session_id: String,
    offset: u64,
    count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkArgs {
    session_id: String,
    href: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    session_id: String,
    query: String,
    mode: SearchMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollArgs {
    session_id: String,
    since_index: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeArgs {
    session_id: String,
    read_id: u64,
    anchor: RangeEnd,
    focus: RangeEnd,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelReadArgs {
    session_id: String,
    read_id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EncodingArgs {
    session_id: String,
    encoding: FileEncoding,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TailArgs {
    session_id: String,
    enabled: bool,
}
