//! Media token registry for the `cmdr-media://` scheme.
//!
//! The viewer renders the content of an *untrusted* file. A hostile file could try to
//! make the webview request `cmdr-media:///etc/ssh/id_rsa`. Validating the requested
//! path against "the session" is weak: the scheme handler can't reliably know which
//! window is asking, and the window already knows its own path. So instead of naming
//! paths in URLs at all, a media open mints a **128-bit unguessable token** and stores
//! `token -> { canonical_path, kind, mime }` here. The frontend builds the URL from the
//! token; the handler resolves token -> path. An unknown token is a 404, and there's no
//! way to name an arbitrary file: the backend only ever exposes files it chose to.
//!
//! **Token lifetime == session lifetime.** [`drop_token`] is called from the single
//! `session::close_session` choke point that frees `SESSIONS`, so both teardown paths
//! (the `viewer_close` IPC and the `WindowEvent::Destroyed` net) free the token too. A
//! closed-window viewer must not leave a live token mapping a real path.
//!
//! **A token can name bytes that don't exist locally yet** ([`DeferredMedia`]): an image
//! in a Markdown document may sit on a phone, inside a zip, or on the web. Its token is
//! minted at render time, and the handler fetches the file on the first request.
//!
//! **A token can name bytes that stay on a volume** ([`VolumeMedia`]): a picture on a share
//! or a server, read by position per request. Dropping the token stops its reads.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use rand::RngExt;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::content_kind::ViewerContentKind;
use super::media_volume::VolumeMedia;
use super::source::ByteSource;

/// What a token resolves to. Cloneable so the scheme handler can resolve under the lock
/// and then work with an owned copy outside it.
#[derive(Debug, Clone)]
pub struct MediaEntry {
    /// The real, already-resolved (tilde-expanded) path the handler will `File::open`.
    pub canonical_path: PathBuf,
    pub kind: ViewerContentKind,
    /// MIME type derived from the file's magic bytes at open time. The handler serves
    /// this as `Content-Type`; it is never derived from the extension.
    pub mime: String,
}

/// A token's bytes that must be fetched before they can be served: pulled off a volume
/// the OS can't open, or downloaded. The scheme handler calls [`fetch`](Self::fetch) on a
/// blocking thread with no total deadline, watching [`idle_for`](Self::idle_for) for a
/// stall, like a viewer open that pulls its file.
pub trait DeferredMedia: Send + Sync {
    /// Blocking. The local file to serve, or `None` (a 404) when the bytes can't be had
    /// or aren't what the token may serve.
    fn fetch(&self) -> Option<MediaEntry>;
    /// How long since the fetch started or last made progress.
    fn idle_for(&self) -> Duration;
    /// The fetch went quiet for too long: stop it at its next chunk.
    fn give_up(&self);
}

/// What a token names.
#[derive(Clone)]
pub enum MediaSource {
    /// A local file, chosen and classified at mint time.
    Ready(MediaEntry),
    /// Bytes to fetch on the first request.
    Deferred(Arc<dyn DeferredMedia>),
    /// Bytes read by position from a volume, per request.
    Volume(Arc<VolumeMedia>),
}

impl MediaSource {
    /// The entry of a token minted over a local file, else `None`.
    pub fn ready(&self) -> Option<&MediaEntry> {
        match self {
            Self::Ready(entry) => Some(entry),
            Self::Deferred(_) | Self::Volume(_) => None,
        }
    }
}

/// Global token -> source map. A `Mutex` (not `RwLock`) because contention is trivial:
/// minting and dropping happen once per viewer open/close (or render), and resolution
/// once per media request.
static MEDIA_TOKENS: LazyLock<Mutex<HashMap<String, MediaSource>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Mints a fresh 128-bit CSPRNG token for `entry`, stores it, and returns the token
/// string (32 lowercase hex chars). The randomness source is `rand`'s OS-backed RNG,
/// matching the "no new RNG crate" constraint.
pub fn mint_token(entry: MediaEntry) -> String {
    mint(MediaSource::Ready(entry))
}

/// Mints a token over bytes a volume serves by position.
pub fn mint_volume_token(media: Arc<VolumeMedia>) -> String {
    mint(MediaSource::Volume(media))
}

/// Mints a token over bytes the handler fetches on the first request.
pub fn mint_deferred_token(source: Arc<dyn DeferredMedia>) -> String {
    mint(MediaSource::Deferred(source))
}

/// Mints a token over whatever `source` names.
pub fn mint(source: MediaSource) -> String {
    let bytes: [u8; 16] = rand::rng().random();
    let token = hex_encode(&bytes);
    MEDIA_TOKENS.lock_ignore_poison().insert(token.clone(), source);
    token
}

/// Resolves a token to its source, or `None` if the token is unknown (never existed or
/// already dropped). The handler maps `None` to a 404.
pub fn resolve_token(token: &str) -> Option<MediaSource> {
    MEDIA_TOKENS.lock_ignore_poison().get(token).cloned()
}

/// Drops the token for `token`, if present, and stops any volume read still running under
/// it (its session is gone, so nobody is waiting). Idempotent.
pub fn drop_token(token: &str) {
    let dropped = MEDIA_TOKENS.lock_ignore_poison().remove(token);
    if let Some(MediaSource::Volume(media)) = dropped {
        media.close();
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Reads an image's pixel dimensions from its header only (no full decode), best-effort.
/// Returns `None` for formats the `image` crate can't parse (HEIC, AVIF, SVG) or on any
/// error. AVIF is in that list because we build `image` without its AVIF decoder; see the
/// `image` dependency comment in `Cargo.toml`.
/// Must stay header-only so it can't extend the viewer open past a quick metadata read.
/// The format is sniffed from the bytes, never the extension, so a PNG named `.txt`
/// still answers (the classifier that got us here decided by magic too).
pub fn read_image_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// How far into a file a header may sit before dimensions give up: a status bar line must
/// not walk a hostile file over the network, and this is what a session's block cache
/// holds, so a re-read costs nothing.
const DIMENSIONS_READ_LIMIT: u64 = 4 * 1024 * 1024;

/// [`read_image_dimensions`] through a session's open file, for a picture on any volume.
/// The source hands out blocks, so an ordinary header costs the open no round trip beyond
/// the one that sniffed it. HEIC and AVIF stop at the format guess.
pub fn read_source_image_dimensions(source: &dyn ByteSource, total_bytes: u64) -> Option<(u32, u32)> {
    let cursor = SourceCursor {
        source,
        len: total_bytes.min(DIMENSIONS_READ_LIMIT),
        position: 0,
    };
    image::ImageReader::new(std::io::BufReader::new(cursor))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// `Read` + `Seek` over a [`ByteSource`]'s first `len` bytes.
struct SourceCursor<'a> {
    source: &'a dyn ByteSource,
    len: u64,
    position: u64,
}

impl std::io::Read for SourceCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let room = self.len.saturating_sub(self.position).min(buf.len() as u64) as usize;
        if room == 0 {
            return Ok(0);
        }
        let read = self
            .source
            .read_at(self.position, &mut buf[..room])
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
        self.position += read as u64;
        Ok(read)
    }
}

impl std::io::Seek for SourceCursor<'_> {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        let target = match to {
            std::io::SeekFrom::Start(offset) => Some(offset),
            std::io::SeekFrom::End(delta) => self.len.checked_add_signed(delta),
            std::io::SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
        };
        self.position = target.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> MediaEntry {
        MediaEntry {
            canonical_path: PathBuf::from("/tmp/x.png"),
            kind: ViewerContentKind::Image,
            mime: "image/png".to_string(),
        }
    }

    #[test]
    fn read_image_dimensions_sniffs_the_bytes_not_the_extension() {
        let dir = crate::test_support::TestDir::new("media_dims");
        let lying = dir.join("notes.txt");
        image::RgbaImage::new(3, 2)
            .save_with_format(&lying, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(read_image_dimensions(&lying), Some((3, 2)));
        let text = dir.join("real.txt");
        std::fs::write(&text, "not a picture").unwrap();
        assert_eq!(read_image_dimensions(&text), None);
    }

    #[test]
    fn mint_returns_32_hex_chars() {
        let token = mint_token(entry());
        assert_eq!(token.len(), 32, "128-bit token == 32 hex chars: {token}");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        drop_token(&token);
    }

    #[test]
    fn mint_then_resolve_roundtrips() {
        let token = mint_token(entry());
        let source = resolve_token(&token).expect("token resolves");
        let resolved = source.ready().expect("a minted entry is ready");
        assert_eq!(resolved.canonical_path, PathBuf::from("/tmp/x.png"));
        assert_eq!(resolved.mime, "image/png");
        drop_token(&token);
    }

    #[test]
    fn tokens_are_unique_across_mints() {
        let a = mint_token(entry());
        let b = mint_token(entry());
        assert_ne!(a, b, "two mints must not collide");
        drop_token(&a);
        drop_token(&b);
    }

    #[test]
    fn dropped_token_resolves_to_none() {
        let token = mint_token(entry());
        drop_token(&token);
        assert!(resolve_token(&token).is_none(), "dropped token must be a miss");
    }

    #[test]
    fn unknown_token_resolves_to_none() {
        assert!(resolve_token("deadbeefdeadbeefdeadbeefdeadbeef").is_none());
    }
}
