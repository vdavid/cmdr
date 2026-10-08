//! Keyed log fields (`path=`, `smb_path=`, `from=`, …): the only way to recognize a path with no mount prefix in
//! front of it. `DETAILS.md` § "Keyed path fields".

use super::RedactionContext;
use super::context::TokenDomain;
use super::identity_token;
use super::names::{split_cmdr_suffix, unescape_debug};
use super::paths::{
    dir_token, has_extension_like_suffix, keeps_parent_name, redact_leaf, redact_media, redact_unix_home,
    redact_unix_system, redact_volumes, redact_windows_home,
};
use super::references::{redact_host, redact_remote_unc, redact_remote_url, redact_scheme_less};
use super::whole_len;
use super::{redactor_regex, split_trailing_noise_with};
use regex::Captures;

/// Path-branch groups: a value one of these claims from its first byte is an absolute path
/// they already know how to redact.
pub(super) const PATH_BRANCHES: &[&str] = &[
    "win_home",
    "unix_home",
    "unix_system",
    "volumes",
    "media",
    "remote_url",
    "unc",
    "url_userinfo",
];

/// Top-level directory names that say where a path lives without saying anything about
/// who owns it. Kept as the first segment of an absolute field value (`path=/private`).
pub(super) const SYSTEM_ROOTS: &[&str] = &[
    "Applications",
    "Library",
    "System",
    "Users",
    "Volumes",
    "bin",
    "cores",
    "dev",
    "etc",
    "home",
    "media",
    "mnt",
    "opt",
    "private",
    "sbin",
    "tmp",
    "usr",
    "var",
];

/// Rewrite a `key=value` path field. Returns (replacement, bytes consumed).
///
/// A quoted value is one complete typed value. An unquoted absolute value a path branch
/// claims ends where the path grammar ends; anything else is walked as a relative path.
pub(super) fn redact_path_field(caps: &Captures<'_>, context: Option<&RedactionContext>) -> (String, usize) {
    let key = caps.name("pf_key").map_or("", |m| m.as_str());
    // `=` in our own fields, `: ` in a `{:?}`-printed struct (`PermissionDenied { path: "…" }`).
    let sep = caps.name("pf_sep").map_or("=", |m| m.as_str());
    let raw = caps.name("pf_value").map_or("", |m| m.as_str());
    let quoted = raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"');
    let quote = if quoted { "\"" } else { "" };
    let head = format!("{key}{sep}{quote}");
    // `path: ` before anything but a quote is prose ("the path: …"), not a field: keep the
    // word and let the scanner carry on from the colon.
    if sep != "=" && !quoted {
        return (key.to_string(), key.len());
    }

    let value = if quoted {
        &raw[1..raw.len() - 1]
    } else {
        end_of_bare_value(raw)
    };
    if value.is_empty() || value == "None" {
        return (head.clone(), head.len());
    }
    if quoted {
        // This value came from a line the scanner may already have transformed. Preserve
        // generated tokens so line redaction remains idempotent.
        let redacted = redact_typed_path(&unescape_debug(value), context, true);
        return (format!("{head}{redacted}{quote}"), whole_len(caps));
    }
    if claimed_by_path_branch(value) {
        // The key says this is a path, so it ends where the path grammar ends, lowercase last
        // word included, and gets the same tokens as its quoted spelling.
        let (path, _) = split_trailing_noise_with(value, false);
        let path = super::references::trim_reference_end(path);
        if !path.is_empty() {
            let redacted = redact_typed_path(path, context, true);
            return (format!("{head}{redacted}"), head.len() + path.len());
        }
        return (head.clone(), head.len());
    }

    let unescaped = if quoted { unescape_debug(value) } else { value.into() };
    let redacted = redact_relative_path(&unescaped, context, true);
    let consumed = head.len() + value.len() + quote.len();
    (format!("{head}{redacted}{quote}"), consumed)
}

/// Redact one complete typed path or URL. Unlike the prose scanner, this function owns the
/// supplied value boundary and therefore never calls `split_trailing_noise` or trims URL
/// punctuation. Extensionless multiword leaves remain one segment all the way to tokenization.
/// `preserve_redacted_segments` is only for values reached through an already-transformed
/// line; raw typed callers must not trust token-looking input.
pub(super) fn redact_typed_path(
    path: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
) -> String {
    let bytes = path.as_bytes();
    if bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && path.get(2..).is_some_and(|tail| tail.starts_with(r"\Users\"))
    {
        return redact_windows_home(path, context);
    }
    if path.starts_with("/Users/") || path.starts_with("/home/") {
        return redact_unix_home(path, context);
    }
    if ["/tmp/", "/var/", "/private/", "/opt/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return redact_unix_system(path, context);
    }
    if path.starts_with("/Volumes/") {
        return redact_volumes(path, context);
    }
    if path.starts_with("/media/") {
        return redact_media(path, context);
    }
    if is_supported_remote_url(path) {
        return redact_remote_url(path, context);
    }
    if path.starts_with(r"\\") {
        return redact_remote_unc(path, context);
    }
    if path.strip_prefix("//").is_some_and(|remainder| {
        remainder
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|authority| authority.contains('@'))
    }) {
        return redact_scheme_less(path, context);
    }
    redact_relative_path(path, context, preserve_redacted_segments)
}

fn is_supported_remote_url(path: &str) -> bool {
    let Some((scheme, remainder)) = path.split_once("://") else {
        return false;
    };
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "sftp" | "ssh" | "webdav" | "s3" | "http" | "https" | "smb"
    ) || (valid_url_scheme(scheme)
        && remainder
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|authority| authority.contains('@')))
}

fn valid_url_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Rewrite one producer-owned identity field while retaining its key and optional wrapper.
pub(super) fn redact_identity_field(caps: &Captures<'_>, context: Option<&RedactionContext>) -> (String, usize) {
    let key = caps.name("if_key").map_or("", |m| m.as_str());
    let raw = caps.name("if_value").map_or("", |m| m.as_str());
    let (prefix, value, suffix) = if raw.starts_with("Some(\"") && raw.ends_with("\")") {
        ("Some(\"", &raw[6..raw.len() - 2], "\")")
    } else {
        ("\"", &raw[1..raw.len() - 1], "\"")
    };
    let token = identity_field_token(key, &unescape_debug(value), context);
    (format!("{key}={prefix}{token}{suffix}"), whole_len(caps))
}

/// The token one identity-field key gives its (unescaped) value.
pub(super) fn identity_field_token(key: &str, value: &str, context: Option<&RedactionContext>) -> String {
    match key {
        "host" | "server" => redact_host(value, context),
        "share" => identity_token("share", TokenDomain::Volume, value, context),
        "volumeId" => identity_token("volume-id", TokenDomain::VolumeId, value, context),
        "volumeName" => identity_token("volume", TokenDomain::Volume, value, context),
        "serverId" => identity_token("server-id", TokenDomain::ServerId, value, context),
        "deviceId" => identity_token("device-id", TokenDomain::DeviceId, value, context),
        _ => value.to_string(),
    }
}

/// Where an unquoted field value really ends. The regex takes the rest of the line; the value
/// stops at the first of: the `{path}: {message}` seam, a `, ` (the next field), or a
/// ` key=` (the next field in `smb2`'s space-separated `from=… to=…`). A `)` left over from
/// `fn(share=…, path=…)` goes too, unless the name opened it (`photo (1).jpg`).
///
/// ⚠️ A comma-space inside an unquoted name ends it early and the rest is handed back
/// unredacted. Only `{}`-printed values can hit that, which is why path fields in our own
/// code are printed with `{:?}`.
pub(super) fn end_of_bare_value(raw: &str) -> &str {
    let mut end = raw.len();
    if let Some(seam) = raw.find(": ") {
        end = seam;
    }
    if let Some(comma) = raw[..end].find(", ") {
        end = comma;
    }
    if let Some((space, _)) = raw[..end]
        .match_indices(' ')
        .find(|(i, _)| starts_with_field_key(&raw[i + 1..end]))
    {
        end = space;
    }
    let mut value = &raw[..end];
    while value.ends_with(')') && value.matches(')').count() > value.matches('(').count() {
        value = &value[..value.len() - 1];
    }
    value.trim_end()
}

/// `ident=` at the start of `s`.
pub(super) fn starts_with_field_key(s: &str) -> bool {
    let ident_len = s
        .char_indices()
        .take_while(|&(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
        .count();
    ident_len > 0 && s[ident_len..].starts_with('=')
}

/// Whether a path branch matches `value` from its very first byte.
pub(super) fn claimed_by_path_branch(value: &str) -> bool {
    redactor_regex().captures(value).is_some_and(|caps| {
        caps.get(0).is_some_and(|m| m.start() == 0) && PATH_BRANCHES.iter().any(|g| caps.name(g).is_some())
    })
}

/// Redact a path no branch has a prefix rule for: share-relative (`docs/a b.pdf`,
/// `docs\a b.pdf`), volume-relative (`/docs/a b.pdf`), or a bare name. Same shape rules as
/// every other path: the leaf keeps its extension, an allowlisted parent keeps its name, the
/// rest collapse. Scanner-owned transformed values may preserve existing tokens for
/// idempotence; raw typed values never do.
pub(super) fn redact_relative_path(
    value: &str,
    context: Option<&RedactionContext>,
    preserve_redacted_segments: bool,
) -> String {
    let sep = if value.contains('/') || !value.contains('\\') {
        '/'
    } else {
        '\\'
    };
    let segments: Vec<&str> = value.split(sep).collect();
    let absolute = value.starts_with(sep);
    let Some(leaf_idx) = segments.iter().rposition(|s| !s.is_empty()) else {
        return value.to_string();
    };
    let mut out = String::with_capacity(value.len());
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            out.push(sep);
        }
        let keep = seg.is_empty()
            || (preserve_redacted_segments && is_redacted_segment(seg))
            || (absolute && i == 1 && SYSTEM_ROOTS.contains(seg));
        if keep {
            out.push_str(seg);
        } else if i == leaf_idx {
            out.push_str(&redact_leaf(seg, has_extension_like_suffix(seg), context));
        } else if i + 1 == leaf_idx && keeps_parent_name(seg) {
            out.push_str(seg);
        } else {
            out.push_str(&dir_token(seg, context));
        }
    }
    out
}

/// The hash-free placeholders unsalted redaction writes (`<dir>`, `<file>.pdf`, …): every
/// kind `identity_token` is called with, plus `<email>`.
const BARE_PLACEHOLDERS: &[&str] = &[
    "dir",
    "file",
    "volume",
    "volume-id",
    "host",
    "share",
    "user",
    "credential",
    "query",
    "fragment",
    "server-id",
    "device-id",
    "email",
    "ipv4-loopback",
    "ipv4-private",
    "ipv4-link-local",
    "ipv4-unspecified",
    "ipv4-public",
    "ipv6-loopback",
    "ipv6-unspecified",
    "ipv6-private",
    "ipv6-link-local",
    "ipv6-public",
    "mtp-owner",
];

/// Whether a path segment is already redacted (`<dir>`, `<file:ab12cd>.pdf`, `$HOME`) or
/// carries nothing to redact (`.`, `..`).
pub(super) fn is_redacted_segment(seg: &str) -> bool {
    if matches!(seg, "$HOME" | "~" | "." | "..") {
        return true;
    }
    let (seg, _) = split_cmdr_suffix(seg);
    let Some(rest) = seg.strip_prefix('<') else {
        return false;
    };
    let Some(close) = rest.find('>') else { return false };
    let (label, tail) = (&rest[..close], &rest[close + 1..]);
    let (kind, hash) = label.split_once(':').unwrap_or((label, ""));
    let kind_ok = !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    // A token carries its hash. Only the unsalted policy's bare placeholders go without one;
    // a real folder named `<anna-kovacs>` is not a token and must not ship as one.
    let hash_ok = if hash.is_empty() {
        BARE_PLACEHOLDERS.contains(&kind)
    } else {
        (hash.len() == 6 || hash.len() == 12) && hash.chars().all(|c| c.is_ascii_hexdigit())
    };
    let tail_ok = tail.is_empty()
        || tail
            .strip_prefix('.')
            .is_some_and(|ext| !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()));
    kind_ok && hash_ok && tail_ok
}
