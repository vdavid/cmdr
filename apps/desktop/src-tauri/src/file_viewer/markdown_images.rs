//! Images in a rendered Markdown document: where each `src` points, and how its bytes
//! reach the viewer.
//!
//! [`resolve_image_src`] is the pure half: it decides, from the `src` alone, whether an
//! image sits beside the document on its volume, on the web, inline, or nowhere the
//! viewer will load. [`VolumeImage`] and [`RemoteImage`] are the fetching half: each is
//! a [`DeferredMedia`] behind a `cmdr-media://` token, so the bytes are fetched only when
//! the webview asks, off the render's path. `DETAILS.md` § "Markdown rendering" has the
//! rules and why.

use cmdr_fs::log_private::LogPrivate;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::debug;

use crate::ignore_poison::IgnorePoison;

use super::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
use super::markdown_links::resolve_relative;
use super::materialize::{PreviewTemp, materialize_markdown_image, write_preview_temp};
use super::media::{DeferredMedia, MediaEntry};
use super::pending_open::{AbandonReason, PendingOpen};

/// The most one image in a document may weigh when it has to be pulled or downloaded.
/// A local file is served in place whatever its size.
pub(super) const IMAGE_CAP_BYTES: u64 = 32 * 1024 * 1024;

/// How long a web image may take, all told.
const REMOTE_IMAGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Where one image's `src` points.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum ImageTarget {
    /// A file on the document's own volume, resolved against the document's folder.
    Local(PathBuf),
    /// An `http(s)` URL, loaded only after the user opts in.
    Remote(String),
    /// A `data:image/…` URI: no request leaves the document, so it renders as written.
    Inline,
    /// Anything else: an absolute path, `file:`, another scheme, or a malformed source.
    Blocked,
}

/// Resolves an image `src` written in `document` (a path on the volume rooted at
/// `volume_root`). Pure: no I/O.
///
/// A relative source resolves like a browser resolves it against the document's
/// folder: `./` and `..` collapse, the query and fragment go, and `%20` decodes. A `..`
/// that would climb past the volume root stops AT the root, so a source can never name
/// a file off the document's volume. Absolute paths and `file:` URLs are blocked: on
/// GitHub `/img.png` means the repository root, which the viewer doesn't know, and a
/// document has no business naming arbitrary files on the machine.
pub(super) fn resolve_image_src(src: &str, document: &Path, volume_root: &Path) -> ImageTarget {
    let src = src.trim();
    if src.is_empty() || (src.starts_with('/') && !src.starts_with("//")) {
        return ImageTarget::Blocked;
    }
    if let Some(rest) = src.strip_prefix("//") {
        return remote(&format!("https://{rest}"));
    }
    match url::Url::parse(src) {
        Ok(url) => match url.scheme() {
            "http" | "https" => ImageTarget::Remote(url.to_string()),
            "data"
                if url
                    .path()
                    .get(..6)
                    .is_some_and(|mime| mime.eq_ignore_ascii_case("image/")) =>
            {
                ImageTarget::Inline
            }
            _ => ImageTarget::Blocked,
        },
        Err(url::ParseError::RelativeUrlWithoutBase) => resolve_relative(src, document, volume_root)
            .map_or(ImageTarget::Blocked, |target| ImageTarget::Local(target.path)),
        Err(_) => ImageTarget::Blocked,
    }
}

fn remote(url: &str) -> ImageTarget {
    match url::Url::parse(url) {
        Ok(url) => ImageTarget::Remote(url.to_string()),
        Err(_) => ImageTarget::Blocked,
    }
}

/// One image's fetch: the watch the stall rule reads, and the file once it's local. The
/// lock is held for the whole fetch, so a second request for the same image waits for
/// the first instead of pulling it again.
struct Fetch {
    watch: PendingOpen,
    done: Mutex<Option<Fetched>>,
}

struct Fetched {
    entry: MediaEntry,
    /// The pulled or downloaded copy, removed when the token (and so this) drops.
    _temp: Option<PreviewTemp>,
}

impl Fetch {
    fn new() -> Self {
        Self {
            watch: PendingOpen::new(),
            done: Mutex::new(None),
        }
    }

    /// The cached entry, or the one `fetch` produces (then cached). `fetch` runs under
    /// the lock with the stall clock restarted: a token minted at render time may be
    /// asked for much later (remote images wait for the user).
    fn get_or_fetch(&self, fetch: impl FnOnce(&PendingOpen) -> Option<Fetched>) -> Option<MediaEntry> {
        let mut done = self.done.lock_ignore_poison();
        if let Some(fetched) = done.as_ref() {
            return Some(fetched.entry.clone());
        }
        self.watch.record_pull(0, None);
        let fetched = fetch(&self.watch)?;
        let entry = fetched.entry.clone();
        *done = Some(fetched);
        Some(entry)
    }

    fn give_up(&self) {
        self.watch.abandon(AbandonReason::StoppedResponding);
    }
}

/// An image on the document's own volume. Served in place when the OS can open it;
/// otherwise (a phone, a zip, a share whose mount went away) pulled into a temp first,
/// through the same materializer a viewer open uses. Either way only a file whose bytes
/// classify as an image is served, so a crafted document can't use an `<img>` to read
/// any other file.
pub(super) struct VolumeImage {
    path: PathBuf,
    volume_id: String,
    /// The document's session closed: stop pulling.
    closed: Arc<AtomicBool>,
    fetch: Fetch,
}

impl VolumeImage {
    pub(super) fn new(path: PathBuf, volume_id: String, closed: Arc<AtomicBool>) -> Self {
        Self {
            path,
            volume_id,
            closed,
            fetch: Fetch::new(),
        }
    }
}

impl DeferredMedia for VolumeImage {
    fn fetch(&self) -> Option<MediaEntry> {
        self.fetch.get_or_fetch(|watch| {
            let temp = materialize_markdown_image(&self.path, &self.volume_id, IMAGE_CAP_BYTES, watch, &self.closed)
                .inspect_err(|e| debug!(target: "cmdr_lib::file_viewer", "markdown image {} not pulled: {e}", LogPrivate::path(&self.path)))
                .ok()?;
            let local = temp.as_ref().map_or_else(|| self.path.clone(), |temp| temp.temp_file.clone());
            let ext = self.path.extension().and_then(|e| e.to_str());
            let entry = image_entry(&local, ext)?;
            Some(Fetched { entry, _temp: temp })
        })
    }

    fn idle_for(&self) -> Duration {
        self.fetch.watch.idle_for()
    }

    fn give_up(&self) {
        self.fetch.give_up();
    }
}

/// An image on the web. Fetched by the backend, never the webview, and only once the
/// user allows remote images for this document: the viewer's CSP keeps `img-src` to
/// `'self' data: cmdr-media:`, and nothing reaches the network before that click.
pub(super) struct RemoteImage {
    url: String,
    allowed: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    fetch: Fetch,
}

impl RemoteImage {
    pub(super) fn new(url: String, allowed: Arc<AtomicBool>, closed: Arc<AtomicBool>) -> Self {
        Self {
            url,
            allowed,
            closed,
            fetch: Fetch::new(),
        }
    }
}

impl DeferredMedia for RemoteImage {
    fn fetch(&self) -> Option<MediaEntry> {
        if !self.allowed.load(Ordering::Relaxed) {
            return None;
        }
        self.fetch.get_or_fetch(|watch| {
            let downloaded = super::bridge::block_on(download(&self.url, IMAGE_CAP_BYTES, watch, &self.closed))
                .inspect_err(|e| debug!(target: "cmdr_lib::file_viewer", "remote markdown image not fetched: {e}"))
                .ok()?;
            let name = format!("image.{}", downloaded.ext);
            let temp = write_preview_temp(&name, &downloaded.bytes, Path::new(&self.url)).ok()?;
            let entry = image_entry(&temp.temp_file, Some(&downloaded.ext))?;
            Some(Fetched {
                entry,
                _temp: Some(temp),
            })
        })
    }

    fn idle_for(&self) -> Duration {
        self.fetch.watch.idle_for()
    }

    fn give_up(&self) {
        self.fetch.give_up();
    }
}

struct Downloaded {
    bytes: Vec<u8>,
    /// What the bytes call themselves, for the classifier's SVG check: `svg` when the
    /// server says `image/svg+xml` (a badge URL often has no extension), else the URL's.
    ext: String,
}

/// Downloads `url`, up to `cap` bytes, recording progress on `watch` and stopping at the
/// next chunk once the document closes or the stall rule gives up. The error is for logs.
async fn download(url: &str, cap: u64, watch: &PendingOpen, closed: &AtomicBool) -> Result<Downloaded, String> {
    let client = cmdr_http::client_builder()
        .timeout(REMOTE_IMAGE_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|len| len > cap) {
        return Err("bigger than the image cap".to_string());
    }
    let says_svg = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|mime| mime.trim_start().to_ascii_lowercase().starts_with("image/svg+xml"));
    let ext = if says_svg {
        "svg".to_string()
    } else {
        url::Url::parse(url)
            .ok()
            .and_then(|url| {
                Path::new(url.path())
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "img".to_string())
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if closed.load(Ordering::Relaxed) || watch.abandoned_error().is_some() {
            return Err("stopped".to_string());
        }
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > cap {
            return Err("bigger than the image cap".to_string());
        }
        watch.record_pull(bytes.len() as u64, None);
    }
    Ok(Downloaded { bytes, ext })
}

/// The entry to serve `local` under, or `None` when it isn't a regular file whose bytes
/// classify as an image. `ext` is the name the image goes by, for the SVG check.
fn image_entry(local: &Path, ext: Option<&str>) -> Option<MediaEntry> {
    use std::io::Read;
    if !std::fs::metadata(local).is_ok_and(|meta| meta.is_file()) {
        return None;
    }
    let mut head = Vec::with_capacity(CLASSIFY_HEAD_LEN);
    std::fs::File::open(local)
        .ok()?
        .take(CLASSIFY_HEAD_LEN as u64)
        .read_to_end(&mut head)
        .ok()?;
    let kind = classify_viewer_content(&head, ext);
    if kind != ViewerContentKind::Image {
        return None;
    }
    Some(MediaEntry {
        canonical_path: local.to_path_buf(),
        kind,
        mime: media_mime(&head, kind)?.to_string(),
    })
}
