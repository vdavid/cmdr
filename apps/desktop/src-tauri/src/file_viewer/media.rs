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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;

use rand::RngExt;

use crate::ignore_poison::IgnorePoison;

use super::content_kind::ViewerContentKind;

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

/// Global token -> entry map. A `Mutex` (not `RwLock`) because contention is trivial:
/// minting and dropping happen once per viewer open/close, and resolution once per
/// media request.
static MEDIA_TOKENS: LazyLock<Mutex<HashMap<String, MediaEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Mints a fresh 128-bit CSPRNG token for `entry`, stores it, and returns the token
/// string (32 lowercase hex chars). The randomness source is `rand`'s OS-backed RNG,
/// matching the "no new RNG crate" constraint.
pub fn mint_token(entry: MediaEntry) -> String {
    let bytes: [u8; 16] = rand::rng().random();
    let token = hex_encode(&bytes);
    MEDIA_TOKENS.lock_ignore_poison().insert(token.clone(), entry);
    token
}

/// Resolves a token to its entry, or `None` if the token is unknown (never existed or
/// already dropped). The handler maps `None` to a 404.
pub fn resolve_token(token: &str) -> Option<MediaEntry> {
    MEDIA_TOKENS.lock_ignore_poison().get(token).cloned()
}

/// Drops the token for `token`, if present. Idempotent.
pub fn drop_token(token: &str) {
    MEDIA_TOKENS.lock_ignore_poison().remove(token);
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
/// Returns `None` for formats the `image` crate can't parse (HEIC, SVG) or on any error.
/// Must stay header-only so it can't extend the viewer open past a quick metadata read.
pub fn read_image_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    image::image_dimensions(path).ok()
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
    fn mint_returns_32_hex_chars() {
        let token = mint_token(entry());
        assert_eq!(token.len(), 32, "128-bit token == 32 hex chars: {token}");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        drop_token(&token);
    }

    #[test]
    fn mint_then_resolve_roundtrips() {
        let token = mint_token(entry());
        let resolved = resolve_token(&token).expect("token resolves");
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
