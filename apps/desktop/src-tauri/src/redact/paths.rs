//! The path-shape rewriters: each path branch keeps its mount/home prefix as a fixed token, and the tail goes
//! through the same leaf and allowlist rules (`redact_path_tail`).

use super::RedactionContext;
use super::SAFE_PARENT_DIR_NAMES;
use super::context::TokenDomain;
use super::names::{ends_with_unicode_escape, split_cmdr_suffix};

pub(super) fn redact_unix_home(path: &str, context: Option<&RedactionContext>) -> String {
    // path like `/Users/<user>/...` or `/home/<user>/...`
    // Strip the `/Users/<user>` prefix and replace with `$HOME`.
    let rest = match path.split('/').nth(3) {
        Some(_) => {
            // find the 3rd `/` and take what follows
            let mut slashes = 0;
            let mut cut = None;
            for (i, ch) in path.char_indices() {
                if ch == '/' {
                    slashes += 1;
                    if slashes == 3 {
                        cut = Some(i);
                        break;
                    }
                }
            }
            cut.map(|i| &path[i..]).unwrap_or("")
        }
        None => "",
    };
    format!("$HOME{}", redact_home_tail(rest, context))
}

pub(super) fn redact_windows_home(path: &str, context: Option<&RedactionContext>) -> String {
    // `C:\Users\<user>\...` → `$HOME\...` (using backslashes to preserve shape)
    // Skip the first 3 `\` separators: `C:` + `\Users` + `\<user>`.
    let mut backslashes = 0;
    let mut cut = None;
    for (i, ch) in path.char_indices() {
        if ch == '\\' {
            backslashes += 1;
            if backslashes == 3 {
                cut = Some(i);
                break;
            }
        }
    }
    let rest = cut.map(|i| &path[i..]).unwrap_or("");
    // Normalize to forward slashes for the tail walker, then convert back.
    let normalized: String = rest.chars().map(|c| if c == '\\' { '/' } else { c }).collect();
    let redacted_tail = redact_home_tail(&normalized, context);
    format!("$HOME{}", redacted_tail.replace('/', "\\"))
}

/// Home folders whose name is a role, not the user's choice: macOS TCC protection keys on
/// them, and Cmdr treats Downloads specially. Report mode keeps them under `$HOME` at any
/// depth. Longest first, so `Library/CloudStorage` wins over `Library`.
const HOME_ROLE_DIRS: &[&str] = &[
    "Library/Application Support",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Downloads",
    "Desktop",
    "Documents",
    "Pictures",
    "Movies",
    "Music",
    "Library",
];

/// Whether an allowlisted parent keeps its name. A home role name only proves its role under
/// `$HOME` (`redact_home_tail`); a remote or nested folder spelled `Documents` is the user's
/// own naming and gets a token.
pub(super) fn keeps_parent_name(seg: &str) -> bool {
    is_safe_parent_dir(seg) && !HOME_ROLE_DIRS.contains(&seg)
}

fn redact_home_tail(tail: &str, context: Option<&RedactionContext>) -> String {
    let body = tail.strip_prefix('/').unwrap_or(tail);
    for role in HOME_ROLE_DIRS {
        if let Some(after) = body.strip_prefix(role)
            && (after.is_empty() || after.starts_with('/'))
        {
            return format!("/{role}{}", redact_path_tail(after, context));
        }
    }
    redact_path_tail(tail, context)
}

pub(super) fn redact_unix_system(path: &str, context: Option<&RedactionContext>) -> String {
    // `/tmp/<rest>`, `/var/<rest>`, `/private/<rest>`, `/opt/<rest>`: keep prefix verbatim,
    // redact everything below it with shape preservation.
    // Find the second `/` (end of the prefix dir), keep `/tmp/` etc., walk the tail.
    let mut slashes = 0;
    let mut tail_start = path.len();
    for (i, ch) in path.char_indices() {
        if ch == '/' {
            slashes += 1;
            if slashes == 2 {
                tail_start = i + 1;
                break;
            }
        }
    }
    let prefix = &path[..tail_start]; // includes trailing `/`
    let tail = &path[tail_start..];
    if tail.is_empty() {
        return prefix.to_string();
    }
    // tail is one or more segments separated by `/`. Reuse redact_path_tail by prepending `/`.
    let redacted = redact_path_tail(&format!("/{tail}"), context);
    // strip the leading `/` we added back since `prefix` already ends in `/`
    format!("{}{}", prefix, redacted.strip_prefix('/').unwrap_or(&redacted))
}

pub(super) fn redact_volumes(path: &str, context: Option<&RedactionContext>) -> String {
    // `/Volumes/<label>/<rest>` → `/Volumes/<volume>/<redacted rest>`
    redact_labeled_mount(path, "/Volumes/", "/Volumes", context)
}

pub(super) fn redact_media(path: &str, context: Option<&RedactionContext>) -> String {
    // `/media/<label>/<rest>` → `/media/<volume>/<redacted rest>`
    redact_labeled_mount(path, "/media/", "/media", context)
}

pub(super) fn redact_labeled_mount(
    path: &str,
    prefix: &str,
    prefix_out: &str,
    context: Option<&RedactionContext>,
) -> String {
    let after = path.strip_prefix(prefix).unwrap_or(path);
    // Label may contain spaces. Find the first `/` to end the label.
    let (label, rest) = after.find('/').map_or((after, ""), |at| (&after[..at], &after[at..]));
    let volume = context.map_or_else(
        || "<volume>".to_string(),
        |context| format!("<volume:{}>", context.token(TokenDomain::Volume, label)),
    );
    format!("{prefix_out}/{volume}{}", redact_path_tail(rest, context))
}

/// Redact the tail of a path (everything after the user/label prefix).
/// Input starts with `/` (or is empty). Output starts with `/` (or is empty).
///
/// Shape preservation: keep the filename's extension and the last directory name if it's
/// in [`SAFE_PARENT_DIR_NAMES`]. Otherwise collapse to `<dir>` / `<file>` (or contextual
/// equivalents when `context` is `Some`).
pub(super) fn redact_path_tail(tail: &str, context: Option<&RedactionContext>) -> String {
    if tail.is_empty() {
        return String::new();
    }
    // tail starts with `/`, strip it for splitting.
    let body = tail.strip_prefix('/').unwrap_or(tail);
    if body.is_empty() {
        return "/".to_string();
    }
    let segments: Vec<&str> = body.split('/').collect();
    if segments.len() == 1 {
        // Single segment under the prefix: could be a dir or a file. We guess based on
        // presence of an extension: segments with a `.X` suffix are files, otherwise dirs.
        let seg = segments[0];
        let is_file = has_extension_like_suffix(seg);
        return format!("/{}", redact_leaf(seg, is_file, context));
    }
    // Walk segments: all but the last are dirs; the last is guessed via the
    // extension heuristic: leaves with `.ext` are files, leaves without are dirs.
    // Don't default leaves to `<file>` unconditionally: directory listings dominate
    // log lines, so they'd read as files in error reports.
    let mut out = String::new();
    let last_idx = segments.len() - 1;
    for (i, seg) in segments.iter().enumerate() {
        out.push('/');
        if i == last_idx {
            let is_file = has_extension_like_suffix(seg);
            out.push_str(&redact_leaf(seg, is_file, context));
        } else if i == last_idx - 1 {
            // Immediate parent dir of the leaf; allowlist check.
            if keeps_parent_name(seg) {
                out.push_str(seg);
            } else {
                out.push_str(&dir_token(seg, context));
            }
        } else {
            // Ancestor dirs: always collapse.
            out.push_str(&dir_token(seg, context));
        }
    }
    out
}

pub(super) fn redact_leaf(seg: &str, is_file: bool, context: Option<&RedactionContext>) -> String {
    redact_leaf_in_domain(seg, is_file, context, TokenDomain::Path)
}

fn redact_leaf_in_domain(seg: &str, is_file: bool, context: Option<&RedactionContext>, domain: TokenDomain) -> String {
    if seg.is_empty() {
        return String::new();
    }
    // `photo.jpg.cmdr-tmp-3f2a…` is `photo.jpg` on its way in: redact the name the temp will
    // become (so both hash alike) and keep Cmdr's own suffix.
    let (name, temp_suffix) = split_cmdr_suffix(seg);
    if !temp_suffix.is_empty() {
        return format!(
            "{}{temp_suffix}",
            redact_leaf_in_domain(name, has_extension_like_suffix(name), context, domain)
        );
    }
    if !is_file {
        return if keeps_parent_name(seg) {
            seg.to_string()
        } else {
            token_for("dir", seg, context, domain)
        };
    }
    // File: keep the extension when it's conservatively one (`conservative_extension`).
    if let Some(ext) = conservative_extension(seg) {
        return format!("{}.{ext}", token_for("file", seg, context, domain));
    }
    token_for("file", seg, context, domain)
}

pub(super) fn dir_token(seg: &str, context: Option<&RedactionContext>) -> String {
    token_for("dir", seg, context, TokenDomain::Path)
}

fn token_for(kind: &str, value: &str, context: Option<&RedactionContext>, domain: TokenDomain) -> String {
    context.map_or_else(
        || format!("<{kind}>"),
        |context| format!("<{kind}:{}>", context.token(domain, value)),
    )
}

pub(super) fn is_safe_parent_dir(name: &str) -> bool {
    SAFE_PARENT_DIR_NAMES.contains(&name)
}

/// True if `seg` looks like a filename with an extension (e.g., `foo.pdf`).
/// False for `Documents`, `.ssh`, `config`, `v0.13.0` (leading digits in ext is fine but
/// we require the dot to be in a reasonable position).
/// Whether this space-separated token looks like the END of a filename: an extension whose
/// first character is a LETTER.
///
/// Stricter than [`has_extension_like_suffix`] on purpose, and the extra letter is doing
/// real work: `01.13.03` in a screenshot timestamp has an "extension" of `03`, so the looser
/// test would cut the name in half and ship ` PM-2.jpeg` verbatim. The cost is that a
/// digit-led extension (`.7z`, `.3gp`) doesn't end the scan, which only means the path may
/// keep a word or two of prose — an over-redaction, never a leak.
pub(super) fn ends_filename(token: &str) -> bool {
    let Some(dot) = token.rfind('.') else { return false };
    let ext = &token[dot + 1..];
    dot > 0
        && !ext.is_empty()
        && ext.len() <= 8
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && ext.starts_with(|c: char| c.is_ascii_alphabetic())
}

/// Whether this token ends a sentence, and with it the path. `/Volumes/naspi-1) claim …`
/// is the shape: the label ends at the `)`, and everything after is prose.
pub(super) fn ends_sentence(token: &str) -> bool {
    matches!(
        token.as_bytes().last(),
        Some(b')' | b';' | b',' | b'.' | b'!' | b'?' | b']' | b'}')
    ) && !ends_with_unicode_escape(token)
}

pub(super) fn has_extension_like_suffix(seg: &str) -> bool {
    conservative_extension(seg).is_some()
}

/// Longer extensions common enough to keep; anything else over five chars is more likely the
/// tail of a name (`Anna.Kovacs`) than a file type.
const KNOWN_LONG_EXTENSIONS: &[&str] = &[
    "sqlite",
    "sqlite3",
    "numbers",
    "keynote",
    "torrent",
    "download",
    "crdownload",
];

/// The segment's extension, when it's conservatively one: after a dot that isn't the first char
/// (no `.ssh`), with at least one letter (no `minutes.2026`), and either lowercase alnum up to
/// five chars, uppercase alnum up to four (camera-style `JPG`, `HEIC`), or a known long one. A
/// dot inside a name (`Anna.Kovacs`) keeps nothing, so the name's tail can't ship as an
/// "extension".
pub(super) fn conservative_extension(seg: &str) -> Option<&str> {
    let dot = seg.rfind('.').filter(|&dot| dot > 0)?;
    let ext = &seg[dot + 1..];
    if ext.is_empty()
        || !ext.chars().all(|c| c.is_ascii_alphanumeric())
        || !ext.chars().any(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let lower = ext.chars().all(|c| !c.is_ascii_uppercase());
    let upper = ext.chars().all(|c| !c.is_ascii_lowercase());
    let keep =
        (lower && ext.len() <= 5) || (upper && ext.len() <= 4) || (lower && KNOWN_LONG_EXTENSIONS.contains(&ext));
    keep.then_some(ext)
}
