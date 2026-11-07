//! Pure reading of an incoming drag's pasteboard: what it carries, and what to call
//! an image that arrives as bare bytes. No AppKit: `macos.rs` hands in the type names.

use super::events::DragPayloadKind;

/// The pasteboard types that mean "a real file is here". Finder publishes both; a
/// legacy source only the second.
const FILE_URL_TYPES: [&str; 2] = ["public.file-url", "NSFilenamesPboardType"];

/// Image flavors Cmdr can write as a file when nothing better is on offer, in the
/// order `clipboard::payload::pick_clipboard_payload` prefers them.
pub(crate) const IMAGE_TYPES: [&str; 3] = ["public.png", "public.tiff", "public.jpeg"];

/// What a drag carries. `types` is the dragging pasteboard's type list,
/// `promise_types` is `NSFilePromiseReceiver.readableDraggedTypes`, and `in_app` says
/// the drag started inside Cmdr (`-[NSDraggingInfo draggingSource]` is non-nil only
/// then).
///
/// Order matters: file URLs win over promises (a source offering both has real files,
/// and wry already hands their paths over), and promises win over image bytes (Chrome
/// puts both on an image drag, and only the promise names the file).
pub(crate) fn classify(types: &[String], promise_types: &[String], in_app: bool) -> DragPayloadKind {
    if in_app {
        return DragPayloadKind::InApp;
    }
    let has = |wanted: &str| types.iter().any(|t| t == wanted);
    if FILE_URL_TYPES.iter().any(|t| has(t)) {
        return DragPayloadKind::FileUrls;
    }
    if promise_types.iter().any(|t| has(t)) {
        return DragPayloadKind::Promises;
    }
    if IMAGE_TYPES.iter().any(|t| has(t)) {
        return DragPayloadKind::ImageData;
    }
    DragPayloadKind::Nothing
}

/// The file name for an image that arrived as bytes: the page URL's last path
/// segment with its extension swapped for `ext`, else `image.<ext>`. Never a path
/// component: separators and dot-only names fall back too.
pub(crate) fn image_file_name(page_url: Option<&str>, ext: &str) -> String {
    let stem = page_url.and_then(url_leaf_stem).unwrap_or_else(|| "image".to_string());
    format!("{stem}.{ext}")
}

/// The last path segment of `url`, decoded and without its extension, when it makes
/// a usable file name.
fn url_leaf_stem(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let leaf = parsed.path_segments()?.next_back()?;
    let decoded = percent_encoding::percent_decode_str(leaf).decode_utf8().ok()?;
    let stem = match decoded.rsplit_once('.') {
        Some((stem, _ext)) if !stem.is_empty() => stem,
        _ => &decoded,
    };
    let stem = stem.trim();
    let usable = !stem.is_empty()
        && !stem.chars().all(|c| c == '.')
        && !stem.contains(['/', '\\', ':', '\0'])
        && stem.len() <= 200;
    usable.then(|| stem.to_string())
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
