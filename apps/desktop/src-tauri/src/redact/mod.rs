//! Shared, path-shape-preserving redactor for log lines, panic messages, and error bundles.
//!
//! The hot path is [`redact_line`], called once per log line by the error reporter.
//! The crash reporter uses [`redact_panic_message`] (a thin alias kept for test parity).
//!
//! # Design
//!
//! One composed regex with named capture groups drives a single pass over each line.
//! The dispatch closure inspects which group matched and calls the appropriate rewriter.
//! This is ~2× faster than chaining `replace_all` calls per pattern and keeps all the
//! redaction rules in one place.
//!
//! # Path-shape preservation
//!
//! `/Users/john/Documents/budget.pdf` becomes `$HOME/Documents/<file>.pdf`. We keep the
//! extension and the immediate parent dir name, but only if that dir name is in the
//! allowlist (`Documents`, `Downloads`, `Desktop`, ...). Unknown parent dirs collapse to
//! `<dir>` so we never leak project-like names (`SecretProjectName`).
//!
//! # Salted mode (per-bundle correlation)
//!
//! [`redact_line`] emits bare `<dir>` / `<file>` tokens, useful but indistinguishable
//! when a log line mentions the same directory twenty times. The error reporter calls
//! [`redact_line_salted`] instead, threading a 16-byte random salt minted at bundle
//! build time. Salted mode emits `<dir:HHHHHH>` / `<file:HHHHHH>` where the 6 hex chars
//! are the first 3 bytes of `blake3(salt || segment)`. Same path → same hash within a
//! bundle, so a triager (or agent) can spot "same dir, mentioned 12 times." Different
//! salt across bundles → no cross-bundle correlation, no rainbow tables.
//!
//! # Coverage
//!
//! See `CLAUDE.md` in this directory for the pattern table and the runbook for adding
//! a new pattern.

use regex::{Captures, Regex};
use std::borrow::Cow;
use std::sync::OnceLock;

#[cfg(test)]
mod tests;

/// Parent directory names we consider safe to keep verbatim in redacted output.
/// Anything else collapses to `<dir>` to avoid leaking project-like names.
const SAFE_PARENT_DIR_NAMES: &[&str] = &[
    "Documents",
    "Downloads",
    "Desktop",
    "Library",
    "src",
    "Pictures",
    "Movies",
    "Music",
    "Public",
    "AppData",
    "Application Support",
];

/// Redact one log line. Hot path: called per line by the error reporter.
///
/// Returns a [`Cow::Borrowed`] when no redaction was needed so we don't allocate
/// on lines like `"Reconciler: switched to live mode"` that have no PII at all.
///
/// Bare `<dir>` / `<file>` tokens. For salted mode (correlatable hashes within a
/// bundle), use [`redact_line_salted`].
#[allow(
    dead_code,
    reason = "Public API for the Phase 4 error reporter; exercised by redact tests."
)]
pub fn redact_line(line: &str) -> Cow<'_, str> {
    redactor_regex().replace_all(line, |caps: &Captures<'_>| dispatch(caps, None))
}

/// Salted variant of [`redact_line`]. Path segments that would collapse to `<dir>`
/// or `<file>` instead emit `<dir:HHHHHH>` / `<file:HHHHHH>` where the 6 hex chars are
/// `blake3(salt || segment)[..3]`. Same input → same output within a single salt;
/// no cross-bundle correlation between different salts.
///
/// The salt is expected to be ≥ 16 bytes of cryptographic random per bundle. Anything
/// shorter is accepted (the hash still correlates) but cross-bundle resistance suffers
/// proportionally.
#[allow(
    dead_code,
    reason = "Public API; wired by the error-report bundle builder once it generates per-bundle salts."
)]
pub fn redact_line_salted<'a>(line: &'a str, salt: &[u8]) -> Cow<'a, str> {
    redactor_regex().replace_all(line, |caps: &Captures<'_>| dispatch(caps, Some(salt)))
}

/// Redact a multi-line text blob. Splits on `\n` and redacts each line independently
/// so regex anchors behave predictably.
#[allow(
    dead_code,
    reason = "Public API for the Phase 4 error reporter; exercised by redact tests."
)]
pub fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        out.push_str(&redact_line(line));
    }
    out
}

/// Redact a panic message. Routes through [`redact_text`] so multi-line payloads (the
/// panic body + chained `caused by:` errors) get every line scrubbed independently.
pub fn redact_panic_message(message: &str) -> String {
    redact_text(message)
}

// --- Internals ---

fn redactor_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Path tail: consecutive path chars, optionally interrupted by single spaces (for
        // labels like "My Backup Drive"). Stops at whitespace-runs, quotes, brackets,
        // and sentence-ending punctuation that's clearly not path content.
        //
        // Tail chars: anything that isn't whitespace, quotes, backticks, angle brackets,
        // or the pipe character. Single spaces between tail chunks are allowed.
        //
        // The `(?x)` verbose flag lets us write this readably.
        Regex::new(
            r#"(?x)
            (?P<win_home>         [A-Za-z] : \\ Users \\ [^\\/\s"'<>|`]+
                                  (?: \\ [^\\\s"'<>|`]+ (?: \x20 [A-Z0-9][^\\\s"'<>|`]* )* )*
            )
            | (?P<unix_home>      / (?: Users | home ) / [^/\s"'<>|`]+
                                  (?: / [^/\s"'<>|`]+ (?: \x20 [A-Z0-9][^/\s"'<>|`]* )* )*
            )
            | (?P<unix_system>    / (?: tmp | var | private | opt ) /
                                  [^/\s"'<>|`]+
                                  (?: / [^/\s"'<>|`]+ (?: \x20 [A-Z0-9][^/\s"'<>|`]* )* )*
            )
            | (?P<volumes>        / Volumes / [^/\s"'<>|`]+ (?: \x20 [^/\s"'<>|`]+ )*
                                  (?: / [^/\s"'<>|`]+ (?: \x20 [A-Z0-9][^/\s"'<>|`]* )* )*
            )
            | (?P<media>          / media / [^/\s"'<>|`]+ (?: \x20 [^/\s"'<>|`]+ )*
                                  (?: / [^/\s"'<>|`]+ (?: \x20 [A-Z0-9][^/\s"'<>|`]* )* )*
            )
            | (?P<smb_uri>        smb:// [^\s"'<>|`]+ )
            | (?P<unc>            \\\\ [A-Za-z0-9_.-]+ (?: \\ [^\\\s"'<>|`]+ (?: \x20 [A-Z0-9][^\\\s"'<>|`]* )* )* )
            | (?P<url_userinfo>   (?P<scheme>[a-zA-Z][a-zA-Z0-9+.-]*) ://
                                  (?P<userinfo>[^\s@/:"'<>|`]+ (?: : [^\s@/"'<>|`]* )? )
                                  @
                                  (?P<host_rest>[^\s"'<>|`]*)
            )
            # Scheme-less userinfo URL: `//user:pass@host[:port][/...]`. The macOS `smbutil`
            # and Linux `smbclient` fallbacks build exactly this shape and a misbehaving
            # server can reflect it in stderr. The regex crate has no lookbehind, so we
            # capture the leading delimiter (start-of-text or a single whitespace char) and
            # re-emit it in the rewriter; this stops us from matching the `//user@host` tail
            # inside a scheme'd `http://user@host` (which `url_userinfo` already handles).
            | (?P<bare_lead>^|\s) (?P<bare_userinfo> //
                                  [^\s@/:"'<>|`]+ (?: : [^\s@/"'<>|`]* )?
                                  @
                                  (?P<bare_host_rest>[^\s"'<>|`]*)
            )
            | (?P<email>          [A-Za-z0-9][A-Za-z0-9._%+-]* @ [A-Za-z0-9][A-Za-z0-9.-]*\.[A-Za-z]{2,} )
            | (?P<mdns>           [A-Za-z0-9][A-Za-z0-9-]{0,62} \. local\b )
            | (?P<ipv6>
                (?:
                  # Full 8-group form: a:b:c:d:e:f:g:h (h is required)
                  \b (?: [0-9A-Fa-f]{1,4} : ){7} [0-9A-Fa-f]{1,4} \b
                  # Compact forms: must have `::` with at least one hex group on at least one side.
                  # `a::b`, `a::`, `::b`, `a:b::c`, `::` alone (not matched, too ambiguous).
                  | \b [0-9A-Fa-f]{1,4} (?: : [0-9A-Fa-f]{1,4} ){0,6} :: (?: [0-9A-Fa-f]{1,4} (?: : [0-9A-Fa-f]{1,4} ){0,6} )? \b
                  | :: [0-9A-Fa-f]{1,4} (?: : [0-9A-Fa-f]{1,4} ){0,6} \b
                  | \b [0-9A-Fa-f]{1,4} (?: : [0-9A-Fa-f]{1,4} ){0,6} ::
                  # Loopback shorthand
                  | :: 1 \b
                )
            )
            | (?P<ipv4>           \b
                                  (?: (?: 25[0-5] | 2[0-4][0-9] | 1[0-9]{2} | [1-9]?[0-9] ) \. ){3}
                                      (?: 25[0-5] | 2[0-4][0-9] | 1[0-9]{2} | [1-9]?[0-9] )
                                \b
            )
            # MTP device name with a possessive owner prefix.
            #   "<Owner>'s Pixel 8 Pro"  → "<mtp-owner>'s Pixel 8 Pro"
            #   "<Owner>'s iPhone"        → "<mtp-owner>'s iPhone"
            # We ONLY match when the owner name is capitalized (so English contractions
            # like "It's a Pixel" don't match: `It` would be the owner candidate, but
            # the model word must follow the apostrophe-`s`-space pattern, and we
            # require the model to be one of a known set).
            # Bare model names without an owner ("Pixel 8 Pro") are intentionally NOT
            # matched; model strings alone aren't identifying and they're useful diag.
            | (?P<mtp_owner>
                \b [A-Z][a-zA-Z]+ ' s
                \x20+
                (?:
                    iPhone | iPad | iPod | Pixel | Galaxy | Samsung | OnePlus
                  | Note | Tablet | Phone | Camera
                )
                (?: \x20+ (?: Pro | Plus | Ultra | Max | Mini | SE | XL ) )?
                (?: \x20+ \d{1,3} )?
                (?: \x20+ (?: Pro | Plus | Ultra | Max | Mini | SE | XL ) )?
                \b
            )
            "#,
        )
        .expect("valid redactor regex")
    })
}

fn dispatch(caps: &Captures<'_>, salt: Option<&[u8]>) -> String {
    if let Some(m) = caps.name("win_home") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_windows_home(path, salt));
    }
    if let Some(m) = caps.name("unix_home") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_unix_home(path, salt));
    }
    if let Some(m) = caps.name("unix_system") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_unix_system(path, salt));
    }
    if let Some(m) = caps.name("volumes") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_volumes(path, salt));
    }
    if let Some(m) = caps.name("media") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_media(path, salt));
    }
    if let Some(m) = caps.name("smb_uri") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_smb_uri(path, salt));
    }
    if let Some(m) = caps.name("unc") {
        let (path, tail) = split_trailing_noise(m.as_str());
        return format!("{}{tail}", redact_unc(path, salt));
    }
    if caps.name("url_userinfo").is_some() {
        // Preserve scheme and everything after the `@`, redact the userinfo.
        let scheme = caps.name("scheme").map(|m| m.as_str()).unwrap_or("");
        let host_rest = caps.name("host_rest").map(|m| m.as_str()).unwrap_or("");
        return format!("{scheme}://<userinfo>@{host_rest}");
    }
    if caps.name("bare_userinfo").is_some() {
        // Scheme-less `//user:pass@host`: drop the userinfo, keep the leading delimiter
        // and everything after the `@`.
        let lead = caps.name("bare_lead").map(|m| m.as_str()).unwrap_or("");
        let host_rest = caps.name("bare_host_rest").map(|m| m.as_str()).unwrap_or("");
        return format!("{lead}//<userinfo>@{host_rest}");
    }
    if caps.name("email").is_some() {
        return "<email>".to_string();
    }
    if caps.name("mdns").is_some() {
        return "<host>.local".to_string();
    }
    if caps.name("ipv6").is_some() {
        return "<ipv6>".to_string();
    }
    if caps.name("ipv4").is_some() {
        return "<ipv4>".to_string();
    }
    if let Some(m) = caps.name("mtp_owner") {
        return redact_mtp_owner(m.as_str());
    }
    // Shouldn't happen: regex matched but no named group. Return verbatim to be safe.
    caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default()
}

/// Replace the possessive owner prefix with `<mtp-owner>`, keep the model words intact.
/// Input is guaranteed to start with `<Owner>'s ` (capital letter, then letters, then
/// `'s`, then one or more spaces) by the regex.
fn redact_mtp_owner(s: &str) -> String {
    // Find the `'s` boundary; everything from there onward is the model phrase.
    // Splitting on `'s` is safe because the regex anchors the apostrophe-s.
    match s.find("'s") {
        Some(i) => {
            // s[i..] starts with "'s", which we want to keep so the redacted output
            // reads naturally ("<mtp-owner>'s Pixel 8 Pro").
            format!("<mtp-owner>{}", &s[i..])
        }
        None => s.to_string(),
    }
}

/// Split a greedy path capture into (path, trailing_noise). The regex allows single spaces
/// inside paths so labels like `/Volumes/My Backup Drive/...` match; that also sweeps up
/// trailing English text like `... .png now` or `... .rs:42:5`. We pull back the tail here
/// before the rewriter runs, then re-emit the tail verbatim in the dispatch output.
///
/// Trimmed:
/// - trailing `:<digits>` groups (line/column markers like `:42:5`)
/// - trailing runs of `\s+<lowercase word>` (sentence continuation like ` now`, ` failed`)
/// - trailing sentence-ending punctuation (`,`, `;`, `.`, `!`, `?`, `)`, `]`, `}`)
fn split_trailing_noise(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    let mut end = bytes.len();

    // First: trim sentence-ending punctuation, one at a time.
    while end > 0 {
        let b = bytes[end - 1];
        if matches!(b, b',' | b';' | b'!' | b'?' | b')' | b']' | b'}') {
            end -= 1;
        } else {
            break;
        }
    }

    // Repeatedly strip `:<digits>` suffixes (e.g. `:42`, `:42:5`).
    loop {
        let mut i = end;
        // consume digits from the right
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i < end && i > 0 && bytes[i - 1] == b':' {
            end = i - 1;
        } else {
            break;
        }
    }

    // Trim a trailing `\s+<lowercase word>` (a sentence continuation).
    // Walk back over word chars, then require at least one whitespace before them.
    {
        let mut i = end;
        while i > 0 && is_word_char(bytes[i - 1]) {
            i -= 1;
        }
        if i < end && i > 0 && bytes[i - 1] == b' ' {
            // Check the word starts with a lowercase letter; capital words are
            // often real path components (`/Volumes/My Backup Drive`).
            if let Some(&first) = bytes.get(i)
                && first.is_ascii_lowercase()
            {
                // Trim the leading space(s) too.
                end = i - 1;
                while end > 0 && bytes[end - 1] == b' ' {
                    end -= 1;
                }
            }
        }
    }

    // Finally, strip a trailing `.` or `,` that was exposed by the above steps.
    while end > 0 {
        let b = bytes[end - 1];
        if matches!(b, b',' | b';' | b'!' | b'?' | b')' | b']' | b'}') {
            end -= 1;
        } else {
            break;
        }
    }

    // SAFETY: we only advance `end` on ASCII byte boundaries.
    (&s[..end], &s[end..])
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// --- Path rewriters ---

fn redact_unix_home(path: &str, salt: Option<&[u8]>) -> String {
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
    format!("$HOME{}", redact_path_tail(rest, salt))
}

fn redact_windows_home(path: &str, salt: Option<&[u8]>) -> String {
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
    let redacted_tail = redact_path_tail(&normalized, salt);
    format!("$HOME{}", redacted_tail.replace('/', "\\"))
}

fn redact_unix_system(path: &str, salt: Option<&[u8]>) -> String {
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
    let redacted = redact_path_tail(&format!("/{tail}"), salt);
    // strip the leading `/` we added back since `prefix` already ends in `/`
    format!("{}{}", prefix, redacted.strip_prefix('/').unwrap_or(&redacted))
}

fn redact_volumes(path: &str, salt: Option<&[u8]>) -> String {
    // `/Volumes/<label>/<rest>` → `/Volumes/<volume>/<redacted rest>`
    redact_labeled_mount(path, "/Volumes/", "/Volumes/<volume>", salt)
}

fn redact_media(path: &str, salt: Option<&[u8]>) -> String {
    // `/media/<label>/<rest>` → `/media/<volume>/<redacted rest>`
    redact_labeled_mount(path, "/media/", "/media/<volume>", salt)
}

fn redact_labeled_mount(path: &str, prefix: &str, prefix_out: &str, salt: Option<&[u8]>) -> String {
    let after = path.strip_prefix(prefix).unwrap_or(path);
    // Label may contain spaces. Find the first `/` to end the label.
    match after.find('/') {
        Some(i) => {
            let rest = &after[i..]; // starts with `/`
            format!("{prefix_out}{}", redact_path_tail(rest, salt))
        }
        None => prefix_out.to_string(),
    }
}

fn redact_smb_uri(uri: &str, salt: Option<&[u8]>) -> String {
    // `smb://host/share/path/file.ext` → `smb://<host>/<share>/<redacted path>`
    let after = uri.strip_prefix("smb://").unwrap_or(uri);
    // split host
    let (_host, rest) = match after.split_once('/') {
        Some(parts) => parts,
        None => return "smb://<host>".to_string(),
    };
    // split share
    let (_share, tail) = match rest.split_once('/') {
        Some(parts) => (parts.0, format!("/{}", parts.1)),
        None => return "smb://<host>/<share>".to_string(),
    };
    format!("smb://<host>/<share>{}", redact_path_tail(&tail, salt))
}

fn redact_unc(unc: &str, salt: Option<&[u8]>) -> String {
    // `\\host\share\path\file.ext` → `\\<host>\<share>\<redacted path>`
    let after = unc.strip_prefix("\\\\").unwrap_or(unc);
    // normalize to forward slashes for reuse, then convert back
    let normalized: String = after.chars().map(|c| if c == '\\' { '/' } else { c }).collect();
    let parts: Vec<&str> = normalized.splitn(3, '/').collect();
    match parts.as_slice() {
        [_host] => r"\\<host>".to_string(),
        [_host, _share] => r"\\<host>\<share>".to_string(),
        [_host, _share, tail] => {
            let redacted = redact_path_tail(&format!("/{tail}"), salt);
            format!(r"\\<host>\<share>{}", redacted.replace('/', "\\"))
        }
        _ => r"\\<host>".to_string(),
    }
}

/// Redact the tail of a path (everything after the user/label prefix).
/// Input starts with `/` (or is empty). Output starts with `/` (or is empty).
///
/// Shape preservation: keep the filename's extension and the last directory name if it's
/// in [`SAFE_PARENT_DIR_NAMES`]. Otherwise collapse to `<dir>` / `<file>` (or salted
/// equivalents when `salt` is `Some`).
fn redact_path_tail(tail: &str, salt: Option<&[u8]>) -> String {
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
        return format!("/{}", redact_leaf(seg, is_file, salt));
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
            out.push_str(&redact_leaf(seg, is_file, salt));
        } else if i == last_idx - 1 {
            // Immediate parent dir of the leaf; allowlist check.
            if is_safe_parent_dir(seg) {
                out.push_str(seg);
            } else {
                out.push_str(&dir_token(seg, salt));
            }
        } else {
            // Ancestor dirs: always collapse.
            out.push_str(&dir_token(seg, salt));
        }
    }
    out
}

fn redact_leaf(seg: &str, is_file: bool, salt: Option<&[u8]>) -> String {
    if seg.is_empty() {
        return String::new();
    }
    if !is_file {
        return if is_safe_parent_dir(seg) {
            seg.to_string()
        } else {
            dir_token(seg, salt)
        };
    }
    // File: try to keep the extension.
    if let Some(dot) = seg.rfind('.') {
        let ext = &seg[dot + 1..];
        // Only preserve "sane" extensions: <= 8 ASCII chars, alnum. Otherwise it's probably
        // a filename with a dot in the stem (e.g., `my.secret.project`), not an extension.
        if !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) && dot > 0 {
            return format!("{}.{ext}", file_token(seg, salt));
        }
    }
    file_token(seg, salt)
}

fn dir_token(seg: &str, salt: Option<&[u8]>) -> String {
    match salt {
        Some(s) => format!("<dir:{}>", short_hash(s, seg)),
        None => "<dir>".to_string(),
    }
}

fn file_token(seg: &str, salt: Option<&[u8]>) -> String {
    match salt {
        Some(s) => format!("<file:{}>", short_hash(s, seg)),
        None => "<file>".to_string(),
    }
}

/// Short, salted, lowercase-hex hash for path segments. 6 hex chars = 3 bytes ≈ 16 M
/// distinct values; collisions are possible but harmless: only correlation within a
/// single bundle's window matters here, and a bundle holds at most low-thousands of
/// distinct path segments. Cross-bundle correlation is prevented by varying the salt.
///
/// Uses SHA-256 (already in our dep tree for license device hashing) rather than
/// pulling in a second hash crate just for this. The hash is overkill for what we
/// need: we only consume the first 3 bytes, but the cost is one allocation per
/// distinct path segment per bundle build, negligible.
fn short_hash(salt: &[u8], segment: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(segment.as_bytes());
    let bytes = hasher.finalize();
    format!("{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])
}

fn is_safe_parent_dir(name: &str) -> bool {
    SAFE_PARENT_DIR_NAMES.contains(&name)
}

/// True if `seg` looks like a filename with an extension (e.g., `foo.pdf`).
/// False for `Documents`, `.ssh`, `config`, `v0.13.0` (leading digits in ext is fine but
/// we require the dot to be in a reasonable position).
fn has_extension_like_suffix(seg: &str) -> bool {
    if let Some(dot) = seg.rfind('.') {
        let ext = &seg[dot + 1..];
        // dot not at position 0 (no `.ssh`) and ext is alnum, <= 8 chars.
        dot > 0 && !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric())
    } else {
        false
    }
}
