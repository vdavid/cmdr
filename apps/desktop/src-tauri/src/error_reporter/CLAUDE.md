# Error reporter

Builds a privacy-redacted zip bundle of recent log files plus a JSON manifest, then (in
prod) ships it to `POST /error-report` on the api server. Used by the user-initiated
"Send error report" flow in Phase 4 and the auto-send flow in Phase 5.

## What we send

Bundle layout:

```
manifest.json          # see BundleManifest
logs/<filename>        # one entry per recent log file in the log dir
logs/<next-filename>   # ...
```

Manifest fields (`BundleManifest`):

- `id`: short ID (`ERR-XXXXX`) generated client-side. **Server may regenerate** — UI
  always shows the server's response ID, never the local one.
- `kind`: `"user"` or `"auto"` (Phase 5).
- `appVersion`, `osVersion`, `arch`: build/platform identifiers.
- `activeSettings`: same booleans/enums the crash reporter ships
  (`indexingEnabled`, `aiProvider`, `mcpEnabled`, `verboseLogging`).
- `userNote` (optional): user-supplied free text. Trimmed; capped at 100 000 chars by the
  Tauri command layer.
- `generatedAt`: ISO 8601 UTC timestamp.

Every log line is run through [`crate::redact::redact_line`](../redact/CLAUDE.md) before
it hits the zip. The redactor handles file paths, hostnames, IPs, emails, URL userinfo,
SMB URIs, and UNC paths. See the redact module for the full pattern table.

## What we never send

- License keys, transaction IDs, device IDs.
- Raw file paths, volume names, SMB credentials.
- Settings registry content beyond the four feature flags above.
- Anything outside the log dir — no app data files, no settings.json.

## Files

| File                       | Purpose                                                                                                       |
| -------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `mod.rs`                   | `build_bundle`, `build_zip`, `generate_short_id`, `upload`, `cap_bundle_to_mb`, `save_bundle_to_disk` (debug); also exports the `log_error!` macro |
| `tests.rs`                 | Unit tests: zip structure, redaction, ID format/uniqueness, capping                                           |
| `auto_dispatcher.rs`       | Flow B: opt-in auto-send on user-visible errors (60 s ± 10 s debounce, 1 MB tail, no retry on failure)        |
| `auto_dispatcher_tests.rs` | Unit tests: debounce, opt-in flag, first-call wins, jitter band, crash-loop interaction                       |

## Two-command frontend split (rationale)

The Tauri commands layer exposes **two** commands:

- `prepare_error_report_preview` returns the manifest + first/last sample lines + total
  size. Used by the dialog to render the preview.
- `send_error_report` re-builds the bundle and uploads it.

Why two commands instead of building once and caching across IPC: the bundle is megabytes
of compressed bytes. Holding it in a Tauri-side `OnceLock` between IPC calls would couple
state across two unrelated commands and risk leaking memory if the user dismisses the
dialog. Re-building is cheap (the heavy work is reading + redacting log lines, which
runs on the blocking pool either way), and the inputs are deterministic enough that the
preview hash matches what'll be uploaded.

## Dev/CI bypass

[`upload`] checks `cfg!(debug_assertions) || std::env::var("CI").is_ok()`. In either
case it returns the locally-generated ID without calling the network. Mirrors the crash
reporter's bypass — same reasoning (no production data pollution from dev runs).

The dialog has an extra "Save bundle to disk (debug)" button in dev that calls
`save_error_report_to_disk` instead, writing the zip to the app data dir for inspection.

## Bundle cap

`cap_bundle_to_mb` is a defensive trimmer used by the Phase 5 auto-dispatcher. Phase 4's
preview dialog doesn't apply a cap — log lines are already bounded by the rotation cap
(`advanced.maxLogStorageMb`, default 200 MB → keep up to four 50 MB files). The server
enforces a 10 MB hard cap anyway.

## Flow B (auto-send on error)

Opt-in via the `updates.errorReports` setting (default off — Flow B sends data without
per-event consent, so the consent has to be up front). When enabled:

1. The `log_error!` macro routes select call sites through
   `auto_dispatcher::on_error_logged(category, message)` in addition to the normal
   `log::error!` emit.
2. The first error in a 60 s window captures `(category, first_message, error_count = 1)`
   and schedules a flush at `now + 60 s ± 10 s of jitter`. Subsequent errors in the same
   window only bump the counter — the first-call metadata is kept verbatim.
3. When the timer fires: build a bundle (`BundleKind::Auto`, user note carries the count
   + first-error preview), trim to a 1 MB tail via `cap_bundle_to_mb`, upload, emit
   `error-report-auto-sent` with the server-issued ID. The frontend listens for that
   event and shows a confirmation toast (see `apps/desktop/src/lib/error-reporter/`).

### Why jitter?

Without jitter, a global outage (DNS, an upstream API) triggers thousands of users to
auto-send at the same `now + 60 s` instant. The ±10 s uniform spread costs nothing on
the client and smears the load over a 20 s window server-side.

### Why no retry on upload failure?

We're already debounced at 60 s. If the network's flaky, the user will hit other errors
soon and the next debounce window will fire normally. Retrying inside a single dispatch
risks flooding the server during real outages, and the user still has Flow A as a manual
safety net.

### Crash-loop interaction (read this!)

If the app exits inside the 60 s debounce window — for example, during a panic — the
spawned flush task is dropped before it fires. **The auto-dispatcher does not flush on
shutdown**, by design.

This is fine because:

- **Panics** route through `crash_reporter`, which writes a JSON file synchronously and
  uploads it on the next launch. That covers the "app died" case end-to-end.
- **Soft errors** that don't kill the app are exactly what the auto-dispatcher exists
  for, and the next `log_error!` call after the next launch will start a fresh window.

If a future scenario shows we're losing important reports here, the simplest fix is to
add a debug-only "flush now" command and let panic hooks call it. Don't add a queue or
on-disk persistence layer — the manual flow is the safety net (matches the FE log
bridge's `beforeunload` semantics: best-effort, no durability guarantees).

### `log_error!` convention

Use `log_error!` instead of `log::error!` at user-visible failure sites — anything that
already produces a user toast or that an end user would describe as "this didn't work."
Skip noisy library-level errors (`smb2`, `nusb`, etc.); the goal is signal, not coverage.

The macro forwards to `log::error!` unconditionally, then calls
`auto_dispatcher::on_error_logged(target, message)` which bails out on a single atomic
load when the opt-in flag is off.

The current set of migrated call sites is small and deliberate; expand it as we discover
new user-visible errors. Do not bulk-migrate.

### AppHandle wiring

The macro can't thread an `AppHandle` through every call site, so
`auto_dispatcher::set_app_handle(handle)` stashes one in a `OnceLock` at app startup
(called from `lib.rs::setup` right after `crash_reporter::init`). If an error fires
before the handle is wired, the debounce window opens normally but the flush task isn't
spawned (no handle to hand to `tauri::async_runtime::spawn`). The state carries a
`flush_spawned` flag for exactly this reason: when `set_app_handle` runs later, it picks
up the orphaned window and spawns the flush task with the remaining time. If the
deadline has already passed, the spawned task fires immediately. The `mark_flush_spawned`
helper plus the late-arrival path in `set_app_handle` race against each other safely —
the loser just bails.

## Gotchas

- The cached `ActiveSettings` snapshot is built lazily on the first `build_bundle` call,
  not at app startup. Settings that change after the first bundle won't appear in
  subsequent reports until restart. This matches the crash reporter's behavior — the
  whole point is to capture the state the user was in when the failure happened.
- `build_zip` uses a `BTreeMap` keyed by filename for deterministic ordering. The live
  `cmdr.log` sorts before rotated `cmdr.log.<timestamp>` siblings because `.` < any
  digit, so the cap iterator hits newest content first.
- Server-side ID generation may differ from the client's local ID. Always trust the
  `id` field in the upload response — that's the one the user reports back to us.
