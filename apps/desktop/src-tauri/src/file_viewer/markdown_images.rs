//! Images in a rendered Markdown document: where each `src` points.
//!
//! [`resolve_image_src`] is pure: it decides, from the `src` alone, whether an image sits
//! beside the document on its volume, on the web, inline, or nowhere the viewer will load.
//! How the bytes are then reached is the host's (`host/images.rs`). `DETAILS.md` §
//! "Markdown rendering" has the rules and why.

use std::path::{Path, PathBuf};

use super::markdown_links::resolve_relative;

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
