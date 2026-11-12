//! What a redacted value looks like: the tokens the redactor writes, recognized so a value that
//! already went through it (or carries nothing to redact) passes through again unchanged. A leaf
//! every rewriter asks, so it imports nothing but `names`.

use super::names::split_cmdr_suffix;

/// The hash-free placeholders unsalted redaction writes (`<dir>`, `<file>.pdf`, …): every
/// kind `identity_token` is called with, plus `<email>`.
const BARE_PLACEHOLDERS: &[&str] = &[
    "redacted",
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
    // a real folder named `<test-person>` is not a token and must not ship as one.
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

/// Whether `token` names a file or a folder, the one kind of leaf worth scrubbing from prose.
pub(super) fn is_name_token(token: &str) -> bool {
    token.starts_with("<file") || token.starts_with("<dir")
}
