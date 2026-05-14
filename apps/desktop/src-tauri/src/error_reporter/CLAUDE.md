# Error reporter

Builds a privacy-redacted zip bundle of recent log files plus a JSON manifest, then (in
prod) ships it to `POST /error-report` on the api server. Used by the user-initiated
"Send error report" flow in Phase 4 and the auto-send flow in Phase 5.

## Convention

**Use `log_error!` at all error-level sites in the desktop crate.** If a failure is
recoverable, expected, or not user-impacting, downgrade to `log::warn!` — don't reach
for `log::error!` to dodge the dispatcher. The error-level threshold IS the auto-report
threshold. The `scripts/check.sh` `log-error-macro` check enforces this and will fail
on any new raw `log::error!` site outside the macro definition itself.

## What we send

Bundle layout:

```
manifest.json          # see BundleManifest
logs/<filename>        # one entry per recent log file in the log dir
logs/<next-filename>   # ...
```

Manifest fields (`BundleManifest`):

- `id`: short ID (`ERR-XXXXX`) generated client-side via [`crate::short_id::generate`]
  (kept as a thin wrapper in `error_reporter::generate_short_id`). The api server
  validates the shape and uses this id as-is — the trailing UUID in the R2 key
  guarantees object uniqueness, so there's no server-side regeneration. The UI shows
  the same id everywhere (dialog preview, toast, server response).
- `kind`: `"user"` or `"auto"` (Phase 5).
- `buildMode`: `"release"` or `"debug"`. Resolved at compile time from
  `cfg!(debug_assertions)` via `BuildMode::current()`. Forwarded to the api server so the
  Discord notification embed prefixes the title with `[DEV]` for debug-build reports —
  triage can keep dev-run reports apart from real production traffic at a glance.
- `appVersion`, `osVersion`, `arch`: build/platform identifiers.
- `activeSettings`: settings snapshot via the `ResolvedSettings` struct — every field
  resolved to its effective value (`null` is never shipped). Includes `indexingEnabled`,
  `aiProvider`, `mcpEnabled`, `mcpPort`, `verboseLogging`, `maxLogStorageMb`,
  `errorReportsEnabled`, `crashReportsEnabled`. Default resolution order per field:
  user-set value → FE-pushed registry default (via `record_settings_defaults`,
  pushed once at FE startup from `settings-store.ts::initializeSettings`) → hardcoded
  fallback in `ResolvedSettings::from_settings`. The hardcoded values are a safety
  net for "error fires before FE init" and unit tests; the registry stays the source
  of truth at runtime.
- `logLevels`: `LogLevelSnapshot` with `stdoutDefault` (startup level), `stdoutCurrent`
  (live atomic), `fileChain` (always `"debug"`), and `stdoutModuleOverrides` (noise
  suppression + `RUST_LOG` directives in insertion order). Lets a triager tell whether
  the absence of a debug line means "didn't happen" or "filtered out."
- `breadcrumbs`: rolling window of the most recent ~50 FE/BE events (oldest
  first). Each entry is `{ at, kind, message, ctx? }`. Populated via the
  `record_breadcrumb` Tauri command (FE wrapper:
  `apps/desktop/src/lib/error-reporter/breadcrumbs.ts::recordBreadcrumb`).
  Backend code can call `error_reporter::breadcrumbs::record(...)` directly.
  The most recent `kind: "command"` entry is the equivalent of the old
  `lastUserAction` field (removed); `handleCommandExecute` pushes one on every
  keyboard / palette / menu dispatch. See "Breadcrumbs" below.
- `userNote` (optional): user-supplied free text. Trimmed; capped at 100 000 chars by the
  Tauri command layer.
- `generatedAt`: ISO 8601 UTC timestamp.

Distinct from `crash_reporter::ActiveSettings`: that struct is the on-disk crash file
format and stays `Option<bool>`-shaped for backward compatibility with crash files
written by older app versions. Manifests are built fresh per bundle and don't have
that constraint, so the resolved shape lives in `error_reporter::ResolvedSettings`.

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
| `mod.rs`                   | Public surface: types (`BundleKind`, `BundleScope`, `BundleManifest`, `ResolvedSettings`, `BuiltBundle`, `UploadResult`), constants (`FLOW_A_BUNDLE_CAP_MB`, `FLOW_B_BUNDLE_CAP_MB`), `generate_short_id`, `upload`, `save_bundle_to_disk` (debug), the `log_error!` macro, and the `log_level_overrides` + `settings_defaults` submodules. Re-exports `build_bundle` and `cap_bundle_to_mb` from the sibling modules. Also holds the cached-settings + log-level-snapshot + `get_os_version` helpers that both pipelines need. |
| `bundle_builder.rs`        | The two build pipelines. `build_bundle` dispatches on scope; `build_bundle_streaming` (Flow A) tail-walks each log file and streams in-window lines through a `CountingCursor`-backed `ZipWriter`, stopping at the cap; `build_bundle_legacy_window` (Flow B) reads each file in full, line-filters, and calls `build_zip`. Owns `PreparedFile`, `CountingCursor`, `zip_dt`, and `load_and_filter_log_file`. |
| `bundle_capper.rs`         | `cap_bundle_to_mb` plus its helpers (`split_into_lines`, `take_tail`, `pick_tail_within_budget`, `read_entry_with_mtime`). Trims log content from the head of the newest file and preserves at least `MIN_TAIL_LINES_OF_NEWEST_FILE` (50) lines of the newest file even if it pushes ~10% over the cap. |
| `tail_walker.rs`           | Reads a log file from the END backward in 64 KB chunks, yields lines newest-first, stops at the timestamp cutoff. Handles long lines that span multiple chunks, lines without leading timestamps (panic continuations), and CRLF defensively. Also owns the shared `parse_leading_iso8601` parser used by `bundle_builder`. |
| `tests.rs`                 | Unit tests: zip structure, redaction, ID format/uniqueness, capping, streaming pipeline                       |
| `auto_dispatcher.rs`       | Flow B: opt-in auto-send on user-visible errors (60 s ± 10 s debounce, 1 MB tail, no retry on failure)        |
| `auto_dispatcher_tests.rs` | Unit tests: debounce, opt-in flag, first-call wins, jitter band, crash-loop interaction                       |
| `breadcrumbs.rs`           | Bounded ring buffer of recent FE/BE triage events (capacity 50). Snapshot is shipped in the manifest.         |

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

## CI bypass

[`upload`] checks `std::env::var("CI").is_ok()` and short-circuits on a hit, returning
the locally-generated ID without calling the network. CI runs shouldn't pollute the
live error-report channel even if a test triggers a report.

Debug builds **do** upload — that's the point of "Send error report" working in dev. The
manifest carries `buildMode: "debug"`, which the api server reads to prefix the Discord
embed title with `[DEV]` so triage can separate dev-run reports from production traffic
at a glance. (Pre-fix-* `upload()` skipped the network in `cfg!(debug_assertions)` too,
which made dev-mode "Send report" silently no-op — confusing and unhelpful.)

The dialog has an extra "Save bundle to disk (debug)" button in dev that calls
`save_error_report_to_disk` instead, writing the zip to the app data dir for inspection.

## Bundle scope and cap

`build_bundle` takes a `BundleScope`:

- `BundleScope::Recent { window }` — Flow A. The default is one hour
  (`BundleScope::flow_a_default()`); the manual-send path uses it. The pipeline is
  **tail-walker + streaming-zip**:
  1. List active log files newest-first via `logging::list_recent_log_files`.
  2. For each file, skip outright if its mtime is older than `now - window`.
  3. Otherwise call `tail_walker::walk_tail`, which reads the file from the END
     backward in 64 KB chunks and yields lines newest-first. The walker stops the
     moment it hits a leading ISO-8601 stamp older than the cutoff.
  4. Each line is redacted on the fly via `redact_line_salted` and streamed straight
     into a `ZipWriter` over a `CountingCursor` (a `Cursor<Vec<u8>>` wrapper holding
     an `AtomicU64` of bytes written through it).
  5. After every line, the running compressed-byte counter is polled. The instant
     it crosses `FLOW_A_BUNDLE_CAP_MB * 1024 * 1024`, streaming stops mid-file.
  6. Walking continues to older rotations only if the cutoff hasn't fired yet AND
     the cap hasn't been reached.

  No `cap_bundle_to_mb` post-pass is needed for this scope (the streaming pipeline
  already enforces the cap). The trailing call in `commands/error_reporter.rs` is
  defense-in-depth in case a manifest-only edge case ever pushes the bundle over.
  Compression is **deflate level 1** — triage logs don't need a 5 % size win at the
  cost of 2× CPU.

- `BundleScope::Window { first_error_at }` — Flow B. The window is
  `[first_error_at - 30 min, now]`. Files whose mtime is older than the lower bound are
  skipped; surviving files are line-filtered by parsing the leading ISO-8601 timestamp
  the file chain writes (see `logging::dispatch::file_timestamp`). Uses the legacy
  "read whole file → redact → BTreeMap → `build_zip` → `cap_bundle_to_mb`" pipeline,
  unchanged from the pre-streaming era. Auto-send runs in a debounced background task
  off the user's hot path, so the simpler shape is fine here.

### Why a tail-walker?

The user-initiated bundle is capped at 1 MB compressed (~19 MB uncompressed). On a
populated log dir (4 × ~50 MB rotated files + 1 live file ≈ 180 MB), the pre-streaming
pipeline read **all 180 MB**, redacted every line into a `Vec<String>`, materialized a
`BTreeMap<filename, PreparedFile>`, then trimmed the head off the result with
`cap_bundle_to_mb`. End-to-end "Preparing preview…" took 30+ seconds and "Save bundle
to disk" sometimes hung visibly. The new path reads only as far back as the cutoff
(typically a few hundred KB at the tail of the live file) and lands in ~100–200 ms.

### Why per-line, not per-file, filtering?

The file-mtime pre-check is a fast-path. The actual decision lives at the line level
because mtime tells you when the file was last *written*, not when each line landed.
On a quiet machine, `cmdr.log` could have been touched 5 minutes ago but most of its
content is days old.

### Why deflate level 1?

Real logs deflate to ~5–10 % of source at level 6 (default). Level 1 lands at ~6–11 %,
i.e. ~10–20 % bigger. For a 1 MB cap, that's 100–200 KB — irrelevant. For CPU, level 1
is 2–3× faster than level 6 in deflate-flate2. Triage cares about latency (the user is
sitting in front of the dialog) more than 100 KB on the wire.

### `cap_bundle_to_mb` (Flow B and post-hoc fallback)

`cap_bundle_to_mb` trims log content from the **head** of the newest file (line by line)
rather than dropping whole files. Always preserves `manifest.json` verbatim and the last
50 lines of the newest file (even if it pushes the cap by ~10%). Used by Flow B (the
auto-dispatcher's bundle is built with the legacy pipeline and trimmed afterwards) and
as a defense-in-depth pass on Flow A in case a future manifest grows large enough to
exceed the cap on its own.

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
   `error-report-auto-sent` with the ID (the same `ERR-XXXXX` the manifest carried — the server validates the shape and echoes it back, never regenerates). The frontend listens for that
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

### Backtrace capture

Every `log_error!` call captures a backtrace via `Backtrace::force_capture()` and emits
it as a **separate debug-level record** under `cmdr_lib::error_reporter::backtrace`.
The fern dispatch tree pins the file chain at Debug regardless of `RUST_LOG`/verbose, so
the backtrace always lands in the log file (and therefore in error report bundles). The
stdout chain's Info default drops it on the floor, keeping the terminal clean. The
error-level message stays a single readable line — pre-fix-* code emitted backtrace as
continuation lines on the error record itself, which spammed the terminal even when no
report was being built. The bundle's per-line window-trim passes through lines without a
parseable leading ISO-8601 timestamp, so the backtrace continuation lines survive into
Flow B reports intact. The redactor scrubs build-machine paths embedded in the symbol
metadata via the same `redact_line` pass every other log line gets.

The auto-dispatcher's `first_message` (which becomes the manifest's `userNote`) sees
only the user-supplied message — the trace stays in the log file. So bundle manifests
stay terse, and triage gets the call site without us having to wire stack-capture into
each error site individually.

`force_capture` ignores `RUST_BACKTRACE`. This is intentional — error report bundles
need stack context regardless of the user's env.

### State snapshot at error time

`auto_dispatcher::on_error_logged` (called from every `log_error!`) also reads the
`cmdr://state` MCP resource and emits the YAML as a debug-level record under
`cmdr_lib::error_reporter::state_snapshot`. Throttled to one per 30 s so an error storm
doesn't fill the file. **Always runs** (regardless of the Flow B opt-in) so manual
"Send error report" bundles built minutes after a failure still have a snapshot.
File-only via the same per-output filtering as the backtrace.

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

## Breadcrumbs

A bounded ring buffer (capacity 50) of recent triage events. Each `Breadcrumb`
is `{ at: ISO-8601, kind: String, message: String, ctx: Option<JSON> }`. The
buffer's snapshot is included in `BundleManifest::breadcrumbs`. Empty buffers
are omitted from JSON via `skip_serializing_if = "Vec::is_empty"`.

Conventions for `kind`:

- `command` — keyboard / palette / menu commands (pushed by `handleCommandExecute`
  in `routes/(main)/command-dispatch.ts`). The most recent entry of this kind is
  the equivalent of the old `lastUserAction` field, which was removed once
  breadcrumbs subsumed it.
- `nav` — navigation transitions (path or pane change).
- `dialog` — open/close of major modals.
- `transfer` — copy / move / delete lifecycle events.
- `error-shown` — friendly error displayed to the user.

Wire new event sources from the FE via
`apps/desktop/src/lib/error-reporter/breadcrumbs.ts::recordBreadcrumb`. Wire
backend events via `error_reporter::breadcrumbs::record(...)`. Both are
fire-and-forget — failures (e.g. lock poisoning, IPC unavailable) are silent
because breadcrumbs are best-effort instrumentation, not a feature.

## Gotchas

- The cached `ActiveSettings` snapshot is built lazily on the first `build_bundle` call,
  not at app startup. Settings that change after the first bundle won't appear in
  subsequent reports until restart. This matches the crash reporter's behavior — the
  whole point is to capture the state the user was in when the failure happened.
- `build_zip` (Flow B legacy path) uses a `BTreeMap` keyed by filename for deterministic
  ordering. The live `cmdr.log` sorts before rotated `cmdr.log.1`/`.2`/... siblings
  because `.` < any digit, so iterating ascending gives newest-first for the log files
  themselves. Flow A's streaming pipeline does NOT use a `BTreeMap` — it iterates
  `list_recent_log_files`'s mtime-sorted output directly and writes entries in walk
  order.
- Per-entry mtimes are set explicitly (manifest = `now`, logs = source-file mtime).
  Without this, the `zip` crate's `SimpleFileOptions::default()` writes 1980-01-01 for
  every entry — extracted bundles look like ancient archives.
- The server uses the client-supplied `id` verbatim — the upload response echoes it.
  Earlier versions regenerated server-side; that was removed because the trailing UUID
  in the R2 key already guarantees uniqueness, and it was confusing to show one id in
  the preview dialog and a different one in the toast.
- The line-timestamp filter (Flow A's tail walker AND Flow B's per-line filter) relies
  on the file chain's ISO-8601 stamp format
  (`YYYY-MM-DDTHH:MM:SS.mmm±HH:MM`, see `logging::dispatch::file_timestamp`). Lines
  without a parseable leading timestamp pass through untouched — they're treated as
  continuation lines of a multi-line record (panic backtraces, state-snapshot YAML).
  The cut boundary always lands on a timestamped line so we never ship a partial
  panic prefix.
- **Tail walker chunk size**: `tail_walker::CHUNK_SIZE` is 64 KB. A single log line
  larger than the chunk (state YAML, deep stack frames) spans multiple chunks; the
  walker accumulates them in a `pending` buffer until a `\n` shows up. Don't
  introduce a max-line-length assumption — backtrace symbol metadata can produce
  ~10 KB lines with no upper bound.
- **Compressed-size tracking** during streaming uses an `AtomicU64` on a wrapping
  `Cursor` (`CountingCursor` in `bundle_builder.rs`). The deflater holds an internal buffer of up
  to ~64 KB that hasn't been flushed to the cursor yet, so the counter is a *lower
  bound* on the eventual on-disk size. Budget conservatively. Don't try to read the
  buffer's `Vec::len()` directly through `ZipWriter::get_mut()` — that's `unsafe` per
  the crate docs and would let the writer's internal seek state and the buffer drift
  out of sync.
- **CountingCursor + ZipWriter ownership**: `ZipWriter::new` takes the `CountingCursor`
  by value. To get the bytes back, `writer.finish()` returns the wrapped cursor; call
  `into_inner()` on it to extract the `Vec<u8>`. Don't try to thread an `&mut Vec<u8>`
  through it — the borrow checker will fight the `Arc<AtomicU64>` you also need.
- Pre-fix-3 logs that started with `HH:MM:SS.mmm` (the legacy stdout-style stamp) won't
  parse as ISO 8601, so they'd pass through both filters as "untimestamped continuation
  lines." For Flow A this means the tail walker won't stop at any of them — it'll keep
  going and get filtered by the file-mtime pre-check instead. For new logs (post
  fix #3), the line filter trims cleanly.
