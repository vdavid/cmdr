//! The path-shape rewriters: each path branch keeps its mount/home prefix as a fixed token, and the tail goes
//! through the same leaf and allowlist rules (`redact_path_tail`).

use super::RedactionContext;
use super::SAFE_PARENT_DIR_NAMES;
use super::context::TokenDomain;
use super::names::{split_cmdr_suffix, unescape_debug};
use super::placeholders::is_placeholder;
use super::tokens::is_redacted_segment;

/// What a path walk makes of the path's last segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Leaf {
    /// A file when it carries a conservative extension, a dir otherwise (`DETAILS.md` §
    /// "Decision: an extensionless leaf reads as `<dir>`"). Every value nobody typed.
    Guess,
    /// A file, extension or not: a producer said so.
    File,
    /// A dir, whatever its name looks like: a producer said so.
    Dir,
}

impl Leaf {
    pub(super) fn is_file(self, seg: &str) -> bool {
        match self {
            Leaf::Guess => has_extension_like_suffix(seg),
            Leaf::File => true,
            Leaf::Dir => false,
        }
    }
}

pub(super) fn redact_unix_home(
    path: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
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
    format!(
        "$HOME{}",
        redact_home_tail(rest, context, preserve_redacted_segments, leaf)
    )
}

pub(super) fn redact_windows_home(
    path: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
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
    let redacted_tail = redact_home_tail(&normalized, context, preserve_redacted_segments, leaf);
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

fn redact_home_tail(
    tail: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
    let body = tail.strip_prefix('/').unwrap_or(tail);
    for role in HOME_ROLE_DIRS {
        if let Some(after) = body.strip_prefix(role)
            && (after.is_empty() || after.starts_with('/'))
        {
            return format!(
                "/{role}{}",
                redact_path_tail(after, context, preserve_redacted_segments, leaf)
            );
        }
    }
    redact_path_tail(tail, context, preserve_redacted_segments, leaf)
}

pub(super) fn redact_unix_system(
    path: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
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
    let redacted = redact_path_tail(&format!("/{tail}"), context, preserve_redacted_segments, leaf);
    // strip the leading `/` we added back since `prefix` already ends in `/`
    format!("{}{}", prefix, redacted.strip_prefix('/').unwrap_or(&redacted))
}

pub(super) fn redact_volumes(
    path: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
    // `/Volumes/<label>/<rest>` → `/Volumes/<volume>/<redacted rest>`
    redact_labeled_mount(path, "/Volumes/", "/Volumes", context, preserve_redacted_segments, leaf)
}

pub(super) fn redact_media(
    path: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
    // `/media/<label>/<rest>` → `/media/<volume>/<redacted rest>`
    redact_labeled_mount(path, "/media/", "/media", context, preserve_redacted_segments, leaf)
}

pub(super) fn redact_labeled_mount(
    path: &str,
    prefix: &str,
    prefix_out: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
    let after = path.strip_prefix(prefix).unwrap_or(path);
    // Label may contain spaces. Find the first `/` to end the label.
    let (label, rest) = after.find('/').map_or((after, ""), |at| (&after[..at], &after[at..]));
    let volume = if preserve_redacted_segments && is_redacted_segment(label) {
        label.to_string()
    } else {
        token_for("volume", label, context, TokenDomain::Volume)
    };
    format!(
        "{prefix_out}/{volume}{}",
        redact_path_tail(rest, context, preserve_redacted_segments, leaf)
    )
}

/// Redact the tail of a path (everything after the user/label prefix).
/// Input starts with `/` (or is empty). Output starts with `/` (or is empty).
///
/// Shape preservation: keep the filename's extension and the last directory name if it's
/// in [`SAFE_PARENT_DIR_NAMES`]. Otherwise collapse to `<dir>` / `<file>` (or contextual
/// equivalents when `context` is `Some`). `leaf` says what the last segment is.
///
/// `preserve_redacted_segments` keeps a segment that's already a token: only for a line the scanner may
/// have rewritten before (`redact_relative_path` has the same rule). Raw typed values never trust
/// token-looking input.
pub(super) fn redact_path_tail(
    tail: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
    leaf: Leaf,
) -> String {
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
        // Single segment under the prefix: could be a dir or a file. Unless a producer said
        // which, we guess based on presence of an extension: `.X` suffix files, otherwise dirs.
        let seg = segments[0];
        if preserve_redacted_segments && is_redacted_segment(seg) {
            return format!("/{seg}");
        }
        return format!("/{}", redact_leaf(seg, leaf.is_file(seg), context));
    }
    // Walk segments: all but the last are dirs; the last is guessed via the
    // extension heuristic: leaves with `.ext` are files, leaves without are dirs.
    // Don't default leaves to `<file>` unconditionally: directory listings dominate
    // log lines, so they'd read as files in error reports.
    let mut out = String::new();
    let last_idx = segments.len() - 1;
    for (i, seg) in segments.iter().enumerate() {
        out.push('/');
        if preserve_redacted_segments && is_redacted_segment(seg) {
            out.push_str(seg);
        } else if i == last_idx {
            out.push_str(&redact_leaf(seg, leaf.is_file(seg), context));
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
    if is_os_junk_name(seg) {
        return seg.to_string();
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
    if is_os_junk_name(seg) {
        return seg.to_string();
    }
    token_for("dir", seg, context, TokenDomain::Path)
}

/// The token for one name, or the span pass's placeholder back unchanged so the span's own token
/// lands there (`markers.rs`).
fn token_for(kind: &str, value: &str, context: Option<&RedactionContext>, domain: TokenDomain) -> String {
    if is_placeholder(value) {
        return value.to_string();
    }
    context.map_or_else(
        || format!("<{kind}>"),
        |context| format!("<{kind}:{}>", context.token(domain, value)),
    )
}

pub(super) fn is_safe_parent_dir(name: &str) -> bool {
    SAFE_PARENT_DIR_NAMES.contains(&name)
}

/// Files the OS or a sync tool keeps in folders for itself. They carry no one's data, and a
/// walk that tripped on one says so, so they ship verbatim. Exact segment matches only:
/// `My.DS_Store.backup` is a name.
const OS_JUNK_NAMES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    ".localized",
    "Icon\r",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
];

/// Whether `seg` is one of [`OS_JUNK_NAMES`], spelled raw or `{:?}`-escaped (`Icon\r`).
pub(super) fn is_os_junk_name(seg: &str) -> bool {
    OS_JUNK_NAMES.contains(&seg) || (seg.contains('\\') && OS_JUNK_NAMES.contains(&&*unescape_debug(seg)))
}

/// How many of `segments` (the ones after `$HOME`) a home role spans: two for
/// `Library/Application Support`, one for `Downloads`, zero when none starts there.
pub(super) fn home_role_len(segments: &[&str]) -> usize {
    HOME_ROLE_DIRS
        .iter()
        .map(|role| role.split('/').collect::<Vec<_>>())
        .find(|role| segments.starts_with(role))
        .map_or(0, |role| role.len())
}

/// True if `seg` looks like a filename with an extension (e.g., `foo.pdf`).
/// False for `Documents`, `.ssh`, `config`, `v0.13.0` (leading digits in ext is fine but
/// we require the dot to be in a reasonable position).
pub(super) fn has_extension_like_suffix(seg: &str) -> bool {
    conservative_extension(seg).is_some()
}

/// Longer extensions common enough to keep; anything else over five chars is more likely the
/// tail of a name (`Test.Person`) than a file type.
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
/// dot inside a name (`Test.Person`) keeps nothing, so the name's tail can't ship as an
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
