# SMB share-listing failure logs raw `smbutil`/`smbclient` output, which can echo a credential-bearing URL

**Severity:** low
**Lens:** F — Security (secret hygiene)
**Confidence:** low

## Location
`apps/desktop/src-tauri/src/network/smb_smbutil.rs:302` (`run_smbutil_view` failure log); same shape on the Linux `smbclient` path in `network/smb_smbclient.rs`.

## What
In `run_smbutil_view`, the non-success branch logs `debug!("smbutil failed: exit={:?}, stderr={}, stdout={}", ...)`. This function is invoked from `list_shares_smbutil_with_auth` where the URL passed to `smbutil` is the credential-bearing one (`//user:password@host`). Cmdr's own call-site logs correctly use the masked `safe_url`, but `smbutil` itself can reflect the full target URL (including userinfo) in its stderr, which is then logged verbatim.

## Why it matters
A debug-level log line could capture the SMB password. The error-report bundle redactor has a `url_userinfo` pattern and an `smb_uri` pattern, but `smbutil`'s error text may print the URL in a shape neither pattern catches (e.g. a bare `//user:pass@host` without scheme). Only reachable at `debug` level and only on an auth-path failure, hence low severity and low confidence (depends on what `smbutil` actually emits).

## Evidence
```rust
// smb_smbutil.rs:302
debug!(
    "smbutil failed: exit={:?}, stderr={}, stdout={}",
    output.status.code(), stderr, stdout
);
```

## Suggested fix
Run `smbutil` stderr/stdout through `redact::redact_text` before logging, or scrub any `:...@` userinfo from the strings before the `debug!`. The Linux `smbclient` path (`-U user%pass` argv) has the same shape and should get the same treatment.

## Notes
The broader secret hygiene was verified strong: `ai/api_keys.rs` logs only key *length*; secret structs carry no secret payload in `Debug`; the error-report bundle only collects redacted `logs/` files. This is the one spot where a subprocess's own output could carry a secret past the redactor's known patterns.
