# Updater string-matches localizable macOS error text

**Severity:** medium **Lens:** C — Error handling **Confidence:** high

## Location

`apps/desktop/src-tauri/src/updater/installer.rs:248-251`

## What

`is_permission_error` decides whether the updater should escalate to admin privileges by substring-matching English-only
error text (`"Permission denied"`, `"Operation not permitted"`). This is exactly the pattern AGENTS.md bans (§ "Critical
rules — No string-matching error or state classification") and the `error-string-match` check is supposed to catch — yet
no `// allowed-error-string-match: ...` opt-out is present, so the check is either silent here or the file slips
through.

## Why it matters

macOS localizes these error messages. On a French/German/Japanese system, `rsync` (or the underlying `EACCES` formatted
by `std::io::Error::Display`) emits the localized message. The substring match returns `false`, the caller treats the
error as fatal instead of escalating, and the update silently fails on `/Applications`. The user sees "Couldn't copy …"
with no admin prompt and no way to install the update. Worse, because `log_error!` doesn't fire here (the function
returns the original error string), this is hard to triage from error reports.

Side effect: it also kills the AppleScript-injection path's only natural mitigation (an early-return before reaching the
dangerous `osascript` call), but that's a defense the high-severity finding shouldn't rely on.

## Evidence

```rust
fn is_permission_error(error: &str) -> bool {
    error.contains("Permission denied") || error.contains("Operation not permitted")
}
```

Caller:

```rust
match sync_bundle(&staged_contents, &bundle_contents) {
    Ok(()) => {}
    Err(e) if is_permission_error(&e) => {
        log::info!("Direct write denied, escalating with admin privileges");
        sync_with_admin_privileges(&staged_contents, &bundle_contents)?;
    }
    Err(e) => return Err(e),
}
```

## Suggested fix

Make `sync_bundle` (and its inner helpers) return a typed error with a `PermissionDenied` variant; bubble the
`io::Error::kind() == ErrorKind::PermissionDenied` from the underlying `fs::copy` / `fs::rename` calls. The caller then
matches on the variant. The current code throws away the `io::Error` and stringifies it at every helper boundary
(`format!("Couldn't copy {} -> {}: {e}", ...)`), so the typed information is lost before `is_permission_error` ever sees
it. Fix the type pipeline, drop the helper.

Until that's done, an `LC_ALL=C` override on the subprocess is the band-aid for the subprocess-driven `rsync` path, but
the direct `fs::copy` path doesn't go through a subprocess at all — there's no shell to localize. So the typed-error
rewrite is the real fix.

## Notes

Same anti-pattern lives in `secrets/keychain_macos.rs::classify_security_error` (separate finding). The
`error-string-match` check appears to miss both; worth investigating whether the check's regex needs broadening.
