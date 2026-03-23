# Crash reporting

## Intention

Cmdr currently has zero visibility into crashes in the wild. When the app crashes, we only learn about it if a user
reports it manually — which almost never happens. The stack overflow crash in `sync_status.rs` (2026-03-22) went
unnoticed until it happened on a dev machine. That's luck, not engineering.

We want lightweight, privacy-respecting crash reports that tell us "version X crashed N times this week, always in
function Y on macOS Z." Enough to prioritize and diagnose, not enough to identify anyone.

**Core principles:**
- **Opt-in, not opt-out.** Consistent with our "no telemetry" stance. The user chooses to send each report.
- **No PII, ever.** No file paths, no usernames, no device IDs, no license keys. Only code symbols and system metadata.
  Panic messages are sanitized to strip file paths before writing the crash file (see milestone 1a).
- **Capture-then-send-next-launch.** You can't reliably send HTTP from a signal handler. Write a crash file, send it
  after the next healthy launch.
- **Radical transparency** (design principle): the user sees exactly what we'd send before they send it.
- **Dev mode: capture only, never send.** Crash files are written in dev mode (useful for testing the feature) but the
  send path is skipped. Dev crashes would pollute production data.

## What we send

**Safe (no PII, highly diagnostic):**
- Full symbolicated backtrace of the crashing thread (function names + offsets — these are code symbols, not user data)
- Exception type + signal (EXC_BAD_ACCESS, SIGABRT, SIGSEGV, etc.)
- Faulting address (distinguishes stack overflow from null deref from use-after-free)
- App version + build number
- macOS version + build
- CPU architecture (arm64 / x86_64)
- App uptime in seconds (startup crash vs steady-state)
- Thread count at crash time
- Panic message, **sanitized** (file paths stripped — `unwrap()` on `io::Error` can embed paths in the message)
- Active feature flags (booleans/enums only: indexing enabled, AI provider type, MCP enabled, verbose logging). Note:
  `ai.provider` reveals the user's choice of AI service (e.g., "openai", "local"). This is non-identifying but could
  be considered sensitive by some users. The "Show report details" view makes this visible before sending.

**Never sent:**
- File paths (contain username, folder names, project names)
- Volume/share names, environment variables, window titles
- License key, transaction ID, device ID
- Register dump, heap contents

## Current state

- The app has **no crash reporting, no telemetry, no Sentry.**
- Outbound calls: license validation (`POST /validate`), update checks (`GET /update-check/:version`), license
  activation (`POST /activate`). That's it.
- Privacy policy explicitly says "no usage telemetry" and lists all network calls. The summary box says "The desktop app
  doesn't phone home."
- `license.getcmdr.com` (Cloudflare Worker) already uses CF Analytics Engine for update checks and downloads — same
  pattern applies here.

## Plan

### Milestone 0: Privacy policy + docs updates

**Intention:** The privacy policy must be updated BEFORE any code that can transmit crash reports ships. Users trust us
because we're transparent. This is a prerequisite to milestone 4 (the server endpoint).

**Privacy policy changes:**

**Summary box** at the top — update from "The desktop app doesn't phone home" to:

```html
<p>
    <strong>The short version:</strong> We collect minimal data. The website uses privacy-friendly
    analytics and collects your email if you subscribe. The desktop app only validates your license
    periodically and checks for updates. You can optionally send crash reports. We don't sell your
    data.
</p>
```

**Section 2, "In the desktop app"** — add a new bullet after the license key bullet:

```html
<li>
    <strong>Crash reports</strong> (opt-in): if you choose to send crash reports, Cmdr sends the
    app version, macOS version, and the location in our code where the crash happened. It doesn't
    include file names, personal data, or anything from your file system. You can enable or disable
    this anytime in Settings > General > Updates.
</li>
```

**Section 2, "no telemetry" paragraph** — update to:

```html
<p>
    The desktop app has <strong>no usage telemetry</strong> — it doesn't report which features you
    use, how often you use them, or anything like that. The only network calls it makes are license
    validation (including the device identifier mentioned above), checking for updates, and sending
    crash reports if you've opted in.
</p>
```

**Section 2, "What we DON'T collect"** — add a clarification:

```html
<li>
    Crash reports, if you opt in, contain only technical diagnostics (code locations, app and
    system version) — never file names, file contents, or anything from your file system.
</li>
```

**Section 3 (legal basis)** — add consent basis for crash reports:

```html
<li>
    <strong>Consent</strong>: Crash reports, if you choose to send them. You can opt out anytime in
    Settings.
</li>
```

**Section 5 (data sharing)** — update the Cloudflare bullet to mention crash reports:

```html
<li>
    <strong>Cloudflare</strong> (hosting) — hosts our license validation, download tracking, and
    crash report ingestion on its global CDN. ...
</li>
```

**Section 7 (data retention)** — add crash reports:

```html
<li>
    <strong>Crash reports</strong> — 90 days (Cloudflare Analytics Engine retention limit)
</li>
```

**Other docs:**
- Update `AGENTS.md` § Debugging to mention crash reports
- Add `src-tauri/src/crash_reporter/CLAUDE.md` with module docs

### Milestone 1: Rust crash capture (backend)

**Intention:** When the app crashes, persist enough information to a file so we can construct a useful report on the
next launch. Two crash categories need separate handlers.

**1a. Panic hook** (Rust `panic!`, `unwrap()`, `expect()` failures)

This runs in regular Rust code, so we can use the full standard library.

- Register a custom panic hook at app startup (in `main.rs` or `lib.rs` setup)
- Capture: `std::backtrace::Backtrace`, panic message, thread name, thread count
- **Sanitize the panic message:** strip anything matching a file path pattern (`/Users/...`, `C:\...`, home dir
  prefixes). `unwrap()` on `io::Error` embeds the path in the error string — this is the most common PII leak vector.
- Write a JSON crash file to the app data dir (`crash-report.json` alongside `settings.json`). Use
  `resolved_app_data_dir()` for dev/prod isolation.
- Then call the default panic hook (so the app still aborts normally)
- The crash file is a simple struct: `{ version: 1, timestamp, signal, panic_message, backtrace_frames[], thread_name,
  thread_count, app_version, os_version, arch, uptime_secs, active_settings {} }`

**1b. Signal handler** (SIGSEGV, SIGBUS, SIGABRT — like the stack overflow crash)

This runs inside a signal handler where almost nothing is async-signal-safe. We must be extremely careful.

- Register signal handlers for SIGSEGV, SIGBUS, SIGABRT at startup
- **Pre-allocate** at startup: a fixed buffer and a pre-opened fd to the crash file path
- In the handler: capture only raw instruction pointer addresses (a `[usize; N]` array on the stack, N=256 max) + the
  signal number + the app version string. Write them as raw bytes to the pre-opened fd. This is async-signal-safe.
- On next launch: read the raw addresses. **Check the app version stored in the crash file against the current binary
  version.** If they differ (user updated between crash and relaunch), skip symbolication and send raw addresses only
  (still useful for grouping by crash site, just not human-readable). If versions match, symbolicate against the
  current binary and format the JSON report.

**Why two handlers:** Panics give us rich, safe access to backtraces and messages. Signals are constrained by
async-signal-safety — we capture minimal raw data and defer the expensive work to next launch.

**Pragmatic note on signal handler complexity:** The async-signal-safe approach (raw addresses + next-launch
symbolication) is the correct approach, but it's also the most complex part of this plan. If it proves too costly to
implement well, a reasonable fallback is to use `backtrace::Backtrace::force_capture()` in the signal handler. This
works in practice on macOS but is technically undefined behavior (it allocates and calls `dladdr`). Start with the
correct approach; fall back only if real implementation issues arise.

**Active settings to include:** `indexing.enabled`, `ai.provider`, `developer.mcpEnabled`, `developer.verboseLogging`.
These are booleans/enums that help correlate crashes with feature combinations without revealing anything about the
user's files or identity. Read from `settings.json` at startup and cache in a static; the signal handler reads the
cached copy.

**Edge cases:**
- **Crash during crash file write:** The next launch must read the crash file defensively. If it fails to parse
  (truncated, corrupt), discard it and delete the file. Don't let a broken crash file block startup.
- **Pre-opened fd staleness:** The fd is opened at startup pointing into the app data dir. If the directory is deleted
  while the app runs, the fd is still valid (Unix semantics) but the file won't be discoverable on next launch. This
  is an acceptable edge case — it requires deliberate user action and only loses one crash report.
- **Crash loop protection:** If the crash file's timestamp is less than 5 seconds before the current launch, skip
  auto-send (even if `updates.crashReports` is true) and show the dialog instead. This prevents a crash-on-startup
  bug from creating an infinite launch-crash-send loop.

**Files to create/modify:**
- New: `src-tauri/src/crash_reporter/mod.rs` — panic hook, signal handler registration, crash file I/O
- New: `src-tauri/src/crash_reporter/symbolicate.rs` — next-launch symbolication of raw addresses
- Modify: `src-tauri/src/lib.rs` — register panic hook + signal handlers in setup

**Tests:**
- Unit test: write a crash file, read it back, verify fields
- Unit test: symbolicate known addresses against the current binary
- Unit test: verify corrupt/truncated crash files are handled gracefully
- Unit test: verify panic message sanitization strips file paths
- The panic hook can be tested by spawning a child process that panics and checking the crash file
- Signal handler testing is inherently limited (can't unit-test a real SIGSEGV safely) — rely on manual testing

### Milestone 2: Next-launch detection + report dialog (frontend + backend)

**Intention:** When the app starts and finds a crash file from a previous session, show the user what happened and let
them choose whether to send the report. The user must see what we'd send — radical transparency.

**2a. Backend: crash file detection**

- New Tauri command: `check_pending_crash_report` — checks if `crash-report.json` exists in the app data dir, reads
  and returns its contents (already formatted as the report struct), or returns `None`. Parses defensively — returns
  `None` and deletes the file if it's corrupt.
- New Tauri command: `dismiss_crash_report` — deletes the crash file without sending
- New Tauri command: `send_crash_report` — POSTs the report to the license server, then deletes the file. **Skipped
  in dev mode** (checks `cfg!(debug_assertions)` or the `CI` env var, same pattern as update checks).

**2b. Frontend: crash dialog**

Show a dialog on startup when a crash report is pending. The dialog follows our confirmation dialog pattern
(title = verb + noun question, body = plain explanation, buttons = outcome verbs).

```
Title: "Send crash report?"

Body:
"Cmdr quit unexpectedly last time. Here's a crash report with details that can help fix this.

It includes the app version, macOS version, and which part of the code crashed — no
file names or personal data."

[▸ Show report details]   ← expandable, shows the JSON we'd send

☐ Always send crash reports

[ Dismiss ]  [ Send report ]
```

**"Show report details"** expands to a scrollable monospace view of the exact JSON payload. This is the radical
transparency: the user sees byte-for-byte what leaves their machine.

**"Always send crash reports"** checkbox: when checked and the user clicks "Send report", it persists
`updates.crashReports` = `true` in the settings store. Future crashes are sent automatically on next launch without
the dialog.

**When `updates.crashReports` is already `true`:** skip the dialog, auto-send the report on startup (unless crash loop
protection kicks in — see milestone 1 edge cases), show a brief non-blocking toast: "Crash report sent — thanks for helping improve Cmdr." The toast should link to the setting so they can opt out if they changed their mind.

**Where this lives:**
- New: `src/lib/crash-reporter/CrashReportDialog.svelte`
- Modify: `src/routes/+layout.svelte` (or wherever the startup flow runs) — check for pending crash report after
  the window loads

### Milestone 3: Settings toggle

**Intention:** Give users a persistent opt-in/out control for automatic crash reporting, placed next to the existing
update check toggle.

**Registry entry:**
```typescript
{
    id: 'updates.crashReports',
    section: ['General', 'Updates'],
    label: 'Send crash reports',
    description: "Automatically send crash reports when Cmdr quits unexpectedly. Includes app version, macOS version, and crash location — no file names or personal data.",
    keywords: ['crash', 'report', 'privacy', 'telemetry', 'bug', 'error'],
    type: 'boolean',
    default: false,
    component: 'switch',
}
```

**Default: `false`** — opt-in only. Consistent with the privacy policy and the "no telemetry" stance.

**Files to modify:**
- `settings-registry.ts` — add the entry above
- `types.ts` — add `'updates.crashReports'` to the `SettingsValues` interface with type `boolean`
- `UpdatesSection.svelte` — add the toggle (same pattern as `updates.autoCheck`)
- `settings-store.ts` — no schema version bump needed (new key with a default is backward-compatible)
- Rust `Settings` struct in `legacy.rs` — add `crash_reports_enabled: bool`, read from key `"updates.crashReports"` in
  `parse_settings` (manual extraction, same pattern as `"developer.mcpEnabled"`)

**Tests:**
- Settings registry test: verify the new setting has valid metadata
- The existing settings validation tests should cover it via the registry

### Milestone 4: License server endpoint

**Intention:** Receive crash reports on the same infrastructure that handles update checks and downloads. No new
services, no new dependencies. **Prerequisite: milestone 0 (privacy policy) must be deployed first.**

**New route: `POST /crash-report`**

- No authentication (same as update checks — we don't want to gate crash reporting on having a license)
- Request body: the JSON crash report struct
- Server-side: validate the payload (reject reports > 64 KB or missing required fields), write to CF Analytics Engine
  (`CRASH_REPORTS` dataset)
- Hash the requester's IP with a daily salt (same pattern as update checks) for rough deduplication
- Return `204 No Content` on success
- Fire-and-forget Analytics Engine write — crash report ingestion should never fail visibly

**CF Analytics Engine schema:**
- `indexes`: `[hashedIp]` (for deduplication)
- `blobs`: `[appVersion, osVersion, arch, signal, topFunction, backtraceTruncated]`
  - `topFunction`: the first app-owned frame in the backtrace (e.g., `sync_status::get_ubiquitous_bool`) — this is the
    primary grouping key for the dashboard
  - `backtraceTruncated`: stringified backtrace, truncated to 5,000 bytes (CF Analytics Engine blob limit is 5,120
    bytes). Truncation cuts from the bottom of the stack (less useful rayon/tokio runtime frames).
- `doubles`: `[1]` (count)

**Wrangler config:**
- New Analytics Engine binding: `CRASH_REPORTS` with dataset `cmdr_crash_reports`
- Add to `wrangler.toml`

**Files to modify:**
- `apps/license-server/src/index.ts` — new route
- `apps/license-server/wrangler.toml` — new AE binding
- Tests for the new route (payload validation, AE write mock)

### Milestone 5: Analytics dashboard integration

**Intention:** Surface crash data in the private analytics dashboard so we can spot crash spikes and identify the most
common crash sites.

- New data source: `src/lib/server/sources/crash-reports.ts` — queries CF Analytics Engine SQL for crash counts grouped
  by `topFunction` and `appVersion`
- New section in the dashboard: "Stability" — shows crash count over time (line chart) and top crash sites (table)
- Same caching pattern as other sources (5 min TTL for 24h/7d, 1 hour for 30d)

This milestone is nice-to-have. The CF Analytics Engine SQL API can be queried manually until the dashboard is built.

## Decisions

**Decision: Opt-in, not opt-out.**
Why: The privacy policy says "no telemetry." Defaulting crash reports to on would contradict that. Opt-in is consistent
with the brand promise. Many users will never opt in — that's fine. Even 10% of crashes reported is infinitely better
than 0%.

**Decision: No Sentry / third-party crash reporter.**
Why: Sentry is a third-party data processor that would need to be listed in the privacy policy, adds a dependency,
and sends far more data than we need. CF Analytics Engine is already in our stack, privacy-preserving by design, and
free at our scale.

**Decision: Separate signal handler vs panic hook (not `backtrace` crate in signal handler).**
Why: The `backtrace` crate's `Backtrace::force_capture()` works in signal handlers on macOS in practice, but it
allocates memory and calls `dladdr`, which are not async-signal-safe. Relying on it is undefined behavior. Raw address
capture is correct and portable. Pragmatic fallback noted in milestone 1b if implementation cost is too high.

**Decision: Show the full JSON in the dialog.**
Why: Design principle — radical transparency. If a privacy-conscious user wants to inspect what leaves their machine,
they can. Most users won't expand it, and that's fine.

**Decision: `updates.crashReports` not `privacy.crashReports`.**
Why: Every existing setting ID uses a namespace that maps to its section in the settings UI (`updates.*` → Updates,
`appearance.*` → Appearance, etc.). Creating a one-off `privacy.*` namespace with a single setting would break this
convention and confuse the Advanced section auto-generation, which groups by ID prefix. The setting lives in the Updates
section, so `updates.*` is the right namespace. If more privacy settings are added later, we can migrate to a dedicated
section.

**Decision: Toast for auto-sent reports, not a dialog.**
Why: If the user already opted in, a blocking dialog on every post-crash launch is annoying. A brief non-blocking
toast respects their choice while keeping them informed.

**Decision: Sanitize panic messages.**
Why: `unwrap()` and `expect()` on `io::Error` embed the file path in the panic message. This path contains the
username and potentially sensitive folder names. Stripping path-like patterns before writing the crash file ensures
no PII leaks even from unexpected panic locations.

**Decision: Dev mode captures crash files but never sends.**
Why: Developers crashing the app during development would pollute production crash data. But writing the crash file
locally is useful for testing the crash reporter itself.

## Testing strategy

- **Unit tests (Rust):** crash file write/read, symbolication, payload validation, panic message sanitization,
  corrupt file handling, version mismatch detection
- **Unit tests (TypeScript):** settings registry validation for the new setting, `SettingsValues` type coverage
- **Integration test:** spawn a child process that panics, verify crash file exists with correct structure
- **Manual test:** trigger a crash in dev mode, verify the dialog appears on next launch, verify the JSON is correct,
  verify send/dismiss both work, verify "Always send" checkbox persists the setting
- **License server tests:** new route handler, payload validation, size limit enforcement, AE write mock
- After each milestone, run `./scripts/check.sh` to verify nothing is broken.

## Execution notes

- Milestone 0 (privacy policy) should ship first — it's a standalone website deploy.
- Milestones 1–4 are sequential (each builds on the previous).
- Milestone 4 requires milestone 0 to be deployed first.
- Milestone 5 (dashboard) is independent and can be done anytime after milestone 4.
- No parallelism needed — this is a clean sequential build-up.
