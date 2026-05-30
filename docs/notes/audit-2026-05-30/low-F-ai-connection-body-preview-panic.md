# `check_ai_connection` byte-slices a response body, panicking on a multibyte UTF-8 boundary

**Severity:** low
**Lens:** F — Security / robustness (also C — panic)
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/ai/manager.rs:706-708`.

## What
On a non-OK/non-401 HTTP status from a user-supplied BYOK `/models` endpoint, the response body is previewed with `format!("{}...", &body[..200])`. Slicing a `String` at a fixed byte index panics if byte 200 falls inside a multibyte UTF-8 sequence — easy with a non-ASCII error body. The same `body_preview` is also surfaced in the `error` field shown in the UI / potentially logged.

## Why it matters
A user-configured AI endpoint that returns a non-ASCII error body (any localized error page, many non-US providers) crashes the command — and the panic itself can trigger a crash report. It's in the audited remote-content path: the endpoint is attacker-influenceable to the degree a user points Cmdr at an untrusted base URL, and the crash is trivially reachable by returning >200 bytes with a multibyte char near offset 200.

## Evidence
```rust
// ai/manager.rs:706-708
let body_preview = if body.len() > 200 {
    format!("{}...", &body[..200])   // ← panics if byte 200 splits a UTF-8 char
```

## Suggested fix
Use a char-safe truncation: `body.chars().take(200).collect::<String>()`, or `&body[..body.floor_char_boundary(200)]`. While there, consider scrubbing `Bearer`-shaped substrings from the preview before surfacing/logging (belt-and-suspenders against a misbehaving proxy reflecting the `Authorization` header in an error body).

## Notes
The wider remote-content posture was verified sound: updater/license/telemetry are https-only in release with timeouts and a pinned minisign key verified before install; BYOK base URLs are scheme-validated and reject plaintext http to non-loopback hosts when a key is set; `withGlobalTauri` is genuinely dev-only. This is the one reachable panic on that surface.
