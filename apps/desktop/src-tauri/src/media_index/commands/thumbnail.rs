//! The search-results grid's thumbnails: mint and drop `cmdr-media://` tokens through the
//! EXISTING viewer preview scheme (plan Decision 5 — never a media_index-produced
//! thumbnail file).

/// Mint a `cmdr-media://` token so the search-results grid can render an image's
/// thumbnail through the EXISTING viewer preview scheme (plan Decision 5 — reuse the
/// preview path, never a media_index-produced thumbnail file). Returns `None` when the
/// path isn't a renderable image (the grid then falls back to a plain tile).
///
/// Token lifetime is the CALLER's here: a viewer session drops its token at the
/// window-close choke point, but the grid has none, so the frontend MUST drop every
/// token via [`media_index_drop_thumbnail_tokens`] when it re-renders or closes, or the
/// token map leaks path mappings. Runs off the IPC thread (a small header read + magic-
/// byte classification).
#[tauri::command]
#[specta::specta]
pub async fn media_index_thumbnail_token(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || mint_image_thumbnail_token(&path))
        .await
        .map_err(|e| format!("thumbnail token task panicked: {e}"))
}

/// Drop `cmdr-media://` tokens the grid minted via [`media_index_thumbnail_token`], once
/// they're no longer displayed. Idempotent; an unknown token is a no-op. Sync + trivial
/// (a map removal per token), so it needn't hop off the IPC thread.
#[tauri::command]
#[specta::specta]
pub fn media_index_drop_thumbnail_tokens(tokens: Vec<String>) {
    for token in &tokens {
        crate::file_viewer::media::drop_token(token);
    }
}

/// Classify `path` by magic bytes and, if it's an image, mint a `cmdr-media://` token
/// for it (reusing the viewer's token registry + scheme). `None` for a non-image or an
/// unreadable file. Local-only: enrichment covers local volumes, so classification trusts the
/// local extension fast-path the viewer uses.
fn mint_image_thumbnail_token(path: &str) -> Option<String> {
    use crate::file_viewer::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
    use crate::file_viewer::media::{self, MediaEntry};

    let p = std::path::Path::new(path);
    let head = read_head_bytes(p, CLASSIFY_HEAD_LEN);
    let ext = p.extension().and_then(|e| e.to_str());
    let kind = classify_viewer_content(&head, ext, true);
    if kind != ViewerContentKind::Image {
        return None;
    }
    let mime = media_mime(&head, kind)
        .unwrap_or("application/octet-stream")
        .to_string();
    Some(media::mint_token(MediaEntry {
        canonical_path: p.to_path_buf(),
        kind,
        mime,
    }))
}

/// Read up to `max` leading bytes for magic-byte classification. Best-effort: an
/// unreadable file yields an empty slice (classified as non-image, so no token).
fn read_head_bytes(path: &std::path::Path, max: usize) -> Vec<u8> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = vec![0u8; max];
    match file.read(&mut buf) {
        Ok(n) => {
            buf.truncate(n);
            buf
        }
        Err(_) => Vec::new(),
    }
}
