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

| File         | Purpose                                                          |
| ------------ | ---------------------------------------------------------------- |
| `mod.rs`     | `build_bundle`, `build_zip`, `generate_short_id`, `upload`, `cap_bundle_to_mb`, `save_bundle_to_disk` (debug) |
| `tests.rs`   | Unit tests: zip structure, redaction, ID format/uniqueness, capping |

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
