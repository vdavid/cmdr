//! Links in a rendered Markdown document, and the relative-reference rule they share
//! with its images.
//!
//! [`resolve_link`] is pure: from an `href` alone it decides whether a click goes to
//! the web or a mail app, to a spot in the same document, to another file on the
//! document's volume (which the viewer opens in the same window), or nowhere.
//! [`resolve_relative`] is the confinement both links and images go through: a
//! reference resolves against the document's folder as a URL rooted at the volume, so
//! it can never name a file off that volume. `DETAILS.md` § "Markdown rendering".

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::markdown_render::ID_PREFIX;

/// What following one link in a rendered Markdown document means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MarkdownLink {
    /// An `http(s)` or `mailto` URL, for the user's browser or mail app.
    External { url: String },
    /// A spot in the same document: the element with this id, or the top when `None`.
    Anchor { element_id: Option<String> },
    /// Another file on the document's volume, opened in the same viewer window, then
    /// scrolled to `element_id` if the link named one.
    File { path: String, element_id: Option<String> },
    /// Nothing the viewer follows: an absolute path, `file:`, another scheme, or empty.
    Blocked,
}

/// A relative reference resolved on the document's volume.
pub(super) struct RelativeTarget {
    pub(super) path: PathBuf,
    /// The fragment, percent-decoded, as written (no `md-` prefix).
    pub(super) fragment: Option<String>,
}

/// Resolves a link `href` clicked in `document` (a path on the volume rooted at
/// `volume_root`). Pure: no I/O, so a missing target is the open's to report.
///
/// A bare `#fragment` arrives already rewritten to the rendered id (`#md-install`,
/// `markdown_render.rs`); a fragment on a relative link gets the prefix here. Absolute
/// paths and `file:` are blocked for the same reason they are for images: on GitHub
/// `/docs/a.md` means the repository root, which the viewer doesn't know.
pub(super) fn resolve_link(href: &str, document: &Path, volume_root: &Path) -> MarkdownLink {
    let href = href.trim();
    if let Some(fragment) = href.strip_prefix('#') {
        return MarkdownLink::Anchor {
            element_id: non_empty(decode(fragment)),
        };
    }
    if href.is_empty() || (href.starts_with('/') && !href.starts_with("//")) {
        return MarkdownLink::Blocked;
    }
    if let Some(rest) = href.strip_prefix("//") {
        return external(&format!("https://{rest}"));
    }
    match url::Url::parse(href) {
        Ok(url) if matches!(url.scheme(), "http" | "https" | "mailto") => {
            MarkdownLink::External { url: url.to_string() }
        }
        Ok(_) => MarkdownLink::Blocked,
        Err(url::ParseError::RelativeUrlWithoutBase) => match resolve_relative(href, document, volume_root) {
            Some(target) => {
                let element_id = target.fragment.and_then(non_empty).map(|id| format!("{ID_PREFIX}{id}"));
                if target.path == document {
                    MarkdownLink::Anchor { element_id }
                } else {
                    MarkdownLink::File {
                        path: target.path.to_string_lossy().into_owned(),
                        element_id,
                    }
                }
            }
            None => MarkdownLink::Blocked,
        },
        Err(_) => MarkdownLink::Blocked,
    }
}

fn external(url: &str) -> MarkdownLink {
    match url::Url::parse(url) {
        Ok(url) => MarkdownLink::External { url: url.to_string() },
        Err(_) => MarkdownLink::Blocked,
    }
}

/// Joins a relative reference onto the document's folder as a URL rooted at the volume,
/// so URL resolution does the dot segments, the query, and the escapes, and its own rule
/// (a `..` at the root stays at the root) is the confinement. `None` when the reference
/// names the volume root itself or the document isn't under `volume_root`.
pub(super) fn resolve_relative(reference: &str, document: &Path, volume_root: &Path) -> Option<RelativeTarget> {
    let folder = document.parent()?.strip_prefix(volume_root).ok()?;
    let base = url::Url::from_directory_path(Path::new("/").join(folder)).ok()?;
    let joined = base.join(reference).ok().filter(|joined| joined.scheme() == "file")?;
    let on_volume = joined.to_file_path().ok()?;
    let relative = on_volume.strip_prefix("/").ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    Some(RelativeTarget {
        path: volume_root.join(relative),
        fragment: joined.fragment().map(decode),
    })
}

fn decode(escaped: &str) -> String {
    String::from_utf8_lossy(&urlencoding::decode_binary(escaped.as_bytes())).into_owned()
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}
