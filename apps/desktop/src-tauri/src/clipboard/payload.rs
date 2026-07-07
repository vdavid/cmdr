//! Clipboard-content payload picking, markdown sniffing, and the content
//! mapping shared by the prod NSPasteboard reader (`pasteboard.rs`) and the E2E
//! mock (`mock.rs`).
//!
//! This is the pure, testable core of "paste clipboard content as a file":
//! `pick_clipboard_payload` applies the flavor precedence (and TIFF→PNG
//! conversion) over an already-read `ClipboardData`, so precedence and
//! conversion are unit-testable without a Tauri runtime or a `MainThreadMarker`.

use super::PastedKind;
use super::store::ClipboardData;

/// The single highest-intent clipboard flavor, resolved by precedence. `Png`
/// covers both a verbatim `public.png` and a `public.tiff` converted to PNG;
/// `Jpeg` is written verbatim as `.jpg`. `Nothing` = no pasteable flavor.
#[derive(Debug)]
pub enum ClipboardPayload {
    Png(Vec<u8>),
    Jpeg(Vec<u8>),
    Pdf(Vec<u8>),
    Text(String),
    Nothing,
}

/// The write plan derived from a payload: the file extension, the toast noun
/// kind, and the bytes to write.
pub struct PastedContent {
    pub ext: &'static str,
    pub kind: PastedKind,
    pub bytes: Vec<u8>,
}

/// Picks the highest-intent flavor from the read clipboard data.
///
/// Precedence: image (`png` > `tiff` > `jpeg`) > pdf > text. A `public.tiff`
/// with no `public.png` is converted to PNG here; a failed conversion falls
/// through to the next flavor. Empty payloads are treated as absent.
pub fn pick_clipboard_payload(data: ClipboardData) -> ClipboardPayload {
    if let Some(png) = data.png.filter(|b| !b.is_empty()) {
        return ClipboardPayload::Png(png);
    }
    if let Some(tiff) = data.tiff.filter(|b| !b.is_empty()) {
        // A `public.tiff` with no PNG is converted here; a failed decode
        // (undecodable TIFF) falls through to the next flavor.
        if let Some(png) = tiff_to_png(&tiff) {
            return ClipboardPayload::Png(png);
        }
    }
    if let Some(jpeg) = data.jpeg.filter(|b| !b.is_empty()) {
        return ClipboardPayload::Jpeg(jpeg);
    }
    if let Some(pdf) = data.pdf.filter(|b| !b.is_empty()) {
        return ClipboardPayload::Pdf(pdf);
    }
    if let Some(text) = data.text.filter(|s| !s.is_empty()) {
        return ClipboardPayload::Text(text);
    }
    ClipboardPayload::Nothing
}

/// Maps a picked payload to its write plan (extension, kind, bytes), running the
/// markdown sniff for text (`.md` vs `.txt`). `Nothing` → `None` (the caller
/// treats this as the "nothing pasteable" no-op).
pub fn payload_to_content(payload: ClipboardPayload) -> Option<PastedContent> {
    match payload {
        ClipboardPayload::Png(bytes) => Some(PastedContent {
            ext: "png",
            kind: PastedKind::Image,
            bytes,
        }),
        ClipboardPayload::Jpeg(bytes) => Some(PastedContent {
            ext: "jpg",
            kind: PastedKind::Image,
            bytes,
        }),
        ClipboardPayload::Pdf(bytes) => Some(PastedContent {
            ext: "pdf",
            kind: PastedKind::Pdf,
            bytes,
        }),
        ClipboardPayload::Text(text) => {
            let ext = if looks_like_markdown(&text) { "md" } else { "txt" };
            Some(PastedContent {
                ext,
                kind: PastedKind::Text,
                bytes: text.into_bytes(),
            })
        }
        ClipboardPayload::Nothing => None,
    }
}

/// Conservative markdown sniffer. Returns `true` only on a strong signal (a
/// fenced code block or an ATX heading at line start) or ≥2 DISTINCT weaker
/// signal KINDS (links `[x](y)`, emphasis, list markers, blockquotes). "Distinct"
/// means distinct kinds, not occurrences: two links are one kind. When in doubt,
/// `false` — a wrong `.md` guess is worse than a plain `.txt`.
pub fn looks_like_markdown(text: &str) -> bool {
    // Sniff only a bounded prefix: markdown signals live early, and this caps the
    // work on a hostile multi-megabyte paste (the full content is still written;
    // only the `.md`-vs-`.txt` extension decision uses the prefix).
    const SNIFF_LIMIT: usize = 64 * 1024;
    let text = if text.len() > SNIFF_LIMIT {
        let mut end = SNIFF_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };

    let mut has_list = false;
    let mut has_quote = false;

    for line in text.lines() {
        // Strong signals, at the very start of a line (a fence or heading
        // mid-line does not count).
        if line.starts_with("```") || line.starts_with("~~~") {
            return true;
        }
        if is_atx_heading(line) {
            return true;
        }
        if is_list_marker(line) {
            has_list = true;
        }
        if line.starts_with("> ") {
            has_quote = true;
        }
    }

    let has_link = has_inline_link(text);
    let has_emphasis = has_emphasis_pair(text);

    [has_link, has_emphasis, has_list, has_quote]
        .into_iter()
        .filter(|&b| b)
        .count()
        >= 2
}

/// `#`..`######` followed by a space, at the very start of the line (CommonMark
/// requires the space, so `#hashtag` is not a heading).
fn is_atx_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(' ')
}

/// `- `, `* `, `+ `, or `<digits>. ` at the line start. The trailing space is
/// what separates a `* ` list marker from `*emphasis*`.
fn is_list_marker(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        return true;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

/// A full `[text](url)` shape somewhere in the text (a bare URL has none, so
/// URL-only text stays `.txt`).
fn has_inline_link(text: &str) -> bool {
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find('[') {
        let open = search_from + rel;
        if let Some(mid_rel) = text[open..].find("](") {
            let after = open + mid_rel + 2;
            if text[after..].contains(')') {
                return true;
            }
        }
        search_from = open + 1;
    }
    false
}

/// A matched emphasis pair — `**bold**`, `__x__`, `*italic*`, or `_x_` — using
/// CommonMark's flanking rules in spirit, tightened for conservatism (cheap
/// heuristic, not a full parser): a pair counts only when the char just INSIDE
/// each delimiter is non-whitespace AND the char just OUTSIDE is not alphanumeric
/// (a word boundary). So spaced `2 * 3 * 4`, unspaced `2*3*4` arithmetic, and
/// intraword `snake_case_name` all stay out of the `.md` bucket, while `*emph*`
/// and `**bold**` at word boundaries still fire. (This is stricter than
/// CommonMark, which allows intraword `*` — a deliberate "when in doubt, `.txt`"
/// call.)
fn has_emphasis_pair(text: &str) -> bool {
    has_emphasis(text, "**") || has_emphasis(text, "__") || has_emphasis(text, "*") || has_emphasis(text, "_")
}

fn has_emphasis(text: &str, delim: &str) -> bool {
    // Single linear pass over the delimiter occurrences (no allocation, no
    // quadratic scan on pathological input like a few MB of repeated `*a`). Keep
    // the FIRST valid opener; any later valid closer with content between it and
    // the opener forms a pair. (If the first valid opener has no valid closer
    // after it, no later opener does either, since closers-after shrink.)
    let dlen = delim.len();
    let mut pending_opener: Option<usize> = None;
    for (pos, _) in text.match_indices(delim) {
        if let Some(open) = pending_opener
            && pos > open + dlen
            && is_emphasis_closer(text, pos, delim)
        {
            return true;
        }
        if pending_opener.is_none() && is_emphasis_opener(text, pos, delim) {
            pending_opener = Some(pos);
        }
    }
    false
}

/// A valid emphasis opener: the char immediately after the delimiter is
/// non-whitespace (kills a trailing/spaced delimiter), and the char before is not
/// alphanumeric (a word boundary, killing intraword `snake_case` and `2*3*4`).
fn is_emphasis_opener(text: &str, pos: usize, delim: &str) -> bool {
    let inside = text[pos + delim.len()..].chars().next();
    if !matches!(inside, Some(c) if !c.is_whitespace()) {
        return false;
    }
    let before = text[..pos].chars().next_back();
    !matches!(before, Some(c) if c.is_alphanumeric())
}

/// A valid emphasis closer: the char immediately before the delimiter is
/// non-whitespace, and the char after is not alphanumeric (a word boundary).
fn is_emphasis_closer(text: &str, pos: usize, delim: &str) -> bool {
    let inside = text[..pos].chars().next_back();
    if !matches!(inside, Some(c) if !c.is_whitespace()) {
        return false;
    }
    let after = text[pos + delim.len()..].chars().next();
    !matches!(after, Some(c) if c.is_alphanumeric())
}

/// Converts TIFF bytes to PNG via `NSBitmapImageRep`. Returns `None` when the
/// bytes aren't decodable as an image. Not main-thread-only (`NSBitmapImageRep`
/// is a data class), so it's callable off-main — which is where it runs (a
/// `spawn_blocking` in the command), so it wraps its objc2 work in an
/// `autoreleasepool`: the blocking thread has no per-runloop pool of its own, and
/// AppKit's autoreleased temporaries would otherwise accumulate on the reused
/// pool thread. The final PNG is copied into an owned `Vec` before the pool drains.
fn tiff_to_png(tiff: &[u8]) -> Option<Vec<u8>> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSData, NSDictionary};

    autoreleasepool(|_pool| {
        let data = NSData::with_bytes(tiff);
        // `imageRepWithData:` decodes the bytes into an NSBitmapImageRep, or nil if
        // they aren't a decodable image (a garbage TIFF returns None here).
        let rep = NSBitmapImageRep::imageRepWithData(&data)?;
        let properties = NSDictionary::new();
        // SAFETY: `-representationUsingType:properties:` re-encodes the decoded bitmap
        // to the given format; `PNG` is a valid `NSBitmapImageFileType` and
        // `properties` is an empty, correctly-typed properties dictionary. It returns
        // an autoreleased `NSData` (nil on failure), retained by objc2's convention.
        // No threading requirement — `NSBitmapImageRep` is a data class, not a UI object.
        let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties) }?;
        Some(png.to_vec())
    })
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod payload_tests;
