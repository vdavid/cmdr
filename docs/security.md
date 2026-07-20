# Security

## withGlobalTauri

The app uses [MCP Server Tauri](https://github.com/hypothesi/mcp-server-tauri) to let AI assistants (Claude Code,
Cursor) control this app: take screenshots, click buttons, and read front-end logs.

The MCP bridge requires `withGlobalTauri: true` which exposes `window.__TAURI__` to the frontend. This would be a huge
security risk in production (untrusted JS could access system APIs, not good), so we enable it **only in development**:

1. **Compile-time exclusion**: The MCP plugin is only registered via `#[cfg(debug_assertions)]` in `lib.rs`
2. **Config separation**: `"withGlobalTauri": false` in `tauri.conf.json` (production). For any non-prod instance, the
   wrapper generates a fresh `tauri.instance.json` under `$TMPDIR` that flips `withGlobalTauri` to `true` (plus sets the
   per-instance identifier and `productName`).
3. **Wrapper script**: `apps/desktop/scripts/tauri-wrapper.ts` writes the generated config and passes it via
   `-c <absolute path>` for `dev` commands. Tauri merges it with `tauri.conf.json` via
   [JSON Merge Patch (RFC 7396)](https://datatracker.ietf.org/doc/html/rfc7396). Prod builds skip the wrapper's instance
   composition entirely, so canonical `tauri.conf.json` (with `withGlobalTauri: false`) governs the bundle.

To avoid security issues in dev mode, always add a condition to **disable** that functionality in dev mode. This way,
malicious websites can't access the system APIs even on your machine.

## Error reports

Cmdr can ship diagnostic bundles (manifest + recent debug-level log tail) to the maintainer when something goes wrong.
Privacy posture:

### Two consent models

- **Flow A: user-initiated.** **Help > Send error report…** or the button on error toasts opens a preview dialog showing
  exactly what's about to be sent (manifest + first 5 / last 20 redacted log lines). Clicking **Send** is the consent,
  no setting required.
- **Flow B: auto-send on error.** Gated by `updates.errorReports` (default **off**). When enabled, user-visible errors
  fire a debounced auto-send (60 s window with ±10 s jitter to avoid lock-step reporting under global outages). The
  toast surfaces the send with **View** (opens the same preview as Flow A) and **Change settings**.

Flow A is unconditional. Flow B is opt-in only.

### Shared redactor

Both flows pass every log line and the manifest through `apps/desktop/src-tauri/src/redact/`. See
[its CLAUDE.md](../apps/desktop/src-tauri/src/redact/CLAUDE.md) for the full pattern catalog and the mandatory
snapshot-tested corpus.

- **Path-shape preserved.** `/Users/john/Documents/budget.pdf` → `$HOME/Documents/<file>.pdf` (extension and known-safe
  parent dir kept, user-identifying parts redacted).
- **Allowlist of safe parent dirs.** `Documents`, `Downloads`, `Desktop`, `Library`, `src`, `Pictures`, `Movies`,
  `Music`, `Public`, `AppData`, `Application Support`. Anything else (for example, `/Users/john/SecretProjectName/...`)
  collapses the parent to `<dir>`.
- **What's redacted:** Unix/Linux/Windows home paths, `/Volumes/<label>`, `/media/<label>`, SMB UNC URIs, bare `*.local`
  hostnames, MTP device names (in log targets), IPv4/IPv6, email addresses, URL userinfo.
- **What's NOT redacted:** module paths (`cmdr_lib::network::smb_client`), filenames inside known-safe dirs (just the
  user-identifying chunks around them), version strings, and anything that looks path-ish but isn't (`Cargo.toml`,
  `0.1.2-alpha`).

The redactor is also used by the crash reporter, so the same rules apply to crash payloads.

### Anonymous reports

No license key, no email, no device ID is attached. The short ID `ERR-XXXXX` (5 chars from the same unambiguous alphabet
as license short codes) is the only correlation handle. Users include it in their bug report when they reach out.

### Server retention

Bundles land in the `cmdr-error-reports` R2 bucket (key: `error-reports/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`). Three
retention layers, in order of aggressiveness:

1. **8/6 GB watermark eviction.** On every upload (in `event.waitUntil(...)`, so the client response isn't blocked) the
   server checks total bytes; if > 8 GB it deletes oldest objects until total ≤ 6 GB. KV-locked to avoid concurrent
   evictors.
2. **Daily cron sweep.** `handleDailyEvictionSweep` recomputes total from R2 ground truth and re-runs eviction. Catches
   KV drift.
3. **R2 lifecycle rule.** 90-day expiration at the bucket level (third safety net if both layers above fail).

### Discord deep links

Each upload triggers a one-line Discord notification to the private `#error-reports` channel with a presigned R2 GET URL
embedded. **TTL: 7 days** (R2's max for presigned URLs). Rationale: only the maintainer has access to the Cmdr Discord
server, and the channel is not shared, so the convenience of a click-to-download link outweighs the theoretical risk of
URL leakage. If access widens later, flip to short-TTL + admin re-mint endpoint (~50 LOC).

## Folder-importance visit signal

The folder-importance subsystem records a lightweight navigation-visit signal to learn which folders the user works in
(feeding the importance scorer). Privacy posture:

- **Local-only, never transmitted.** Visits live in the per-volume `importance.db` on the user's disk (a disposable
  cache, purged with the index). Nothing about visits is sent anywhere — not to the maintainer, not to PostHog
  analytics, not in error reports. It's not telemetry.
- **Counts and timestamps only, no content.** Each row is `(folder path, visit count, last-visit seconds)`. No file
  contents, no file names beyond the folder path itself, no per-file data.
- **Background-scored volumes only.** The local disk and SMB shares record visits (the volumes importance scores);
  on-demand-only MTP devices and unregistered volumes don't. The signal is still the same counts-and-timestamps shape on
  every volume, always local to the user's disk.
- **Fire-and-forget and failure-silent.** The `record_visit` command never blocks or breaks navigation, and a visit that
  can't be written is silently dropped. Recording a visit is best-effort, never load-bearing.

## Operation log

The operation log (`operation_log/`) journals every file mutation to a durable `operation-log.db` in the app data dir,
so the user can search their history and roll operations back. It is itself sensitive — effectively a map of the user's
file activity (what was copied, moved, trashed, renamed, and where). Privacy posture:

- **Local-only, never transmitted.** The journal lives on the user's disk and is never sent anywhere — not to the
  maintainer, not to PostHog analytics, not in crash or error reports. It's not telemetry. It IS backed up by Time
  Machine like any Application Support file (deliberately — restoring it restores undo-ability); retention (default 3
  GB) bounds that, and a future "exclude from backups" toggle is the identified escape hatch.
- **File-activity metadata, not contents.** Rows hold operation kind, initiator, timestamps, per-item paths/names,
  sizes, mtimes, and outcomes — never file contents.
- **The journal never compromises the operation.** Capture rides a bounded channel that blocks briefly under
  backpressure (lossless) and drops a single row on a DB error rather than failing the file op; the finalize-time
  completeness check then degrades that op to "can't undo" or "search marked partial" rather than silently
  under-reversing or claiming false coverage.

## Secret scanning (GitGuardian)

GitGuardian watches the repo and opens an incident per suspected secret. Two independent surfaces, configured
separately:

- **`.gitguardian.yaml`** (repo root) is the ggshield config: it covers CI and pre-commit runs. `secret.ignored-paths`
  excludes `apps/desktop/src/lib/intl/messages/**`, because every locale spells "password" in its own language for the
  archive-password dialog and the Generic Password detector flags each translation.
- **The dashboard's workspace exclusions** are what the GitHub App's realtime scanning honors; it does NOT read
  `.gitguardian.yaml`. Mirror any path added above under Settings → Secrets detection → Custom exclusions in workspace
  `677563`, or the incidents keep arriving. The API does not expose exclusions on this plan, so this step is manual.

Triage order for a real hit is rotate at the provider first, then remove from the repo, then resolve the incident: git
history keeps the old value, so resolving without rotating fixes nothing. Access details and API recipes live in
`~/Dropbox/obsidian/agents/tooling/gitguardian.md`.

## Ask Cmdr agent egress (to the user's LLM provider)

Ask Cmdr is the one subsystem that deliberately sends user data OFF the Mac — to the AI provider the user configured,
with their own API key. Privacy posture:

- **Consent-gated, fail-closed.** Every send checks `agent::consent::has_current_consent` in the backend before it
  resolves the LLM; an absent or stale acceptance refuses the send (not just a UI affordance). The consent copy
  (`askCmdr.consent.*`) enumerates exactly what egresses; bump `CONSENT_COPY_VERSION` when that set changes so users
  re-accept.
- **Read-only, no arbitrary file contents.** The agent has no write tool and no tool that reads a file's bytes. What
  reaches the provider is file/folder names, paths, sizes, dates, and the app-state envelope (spec §2.1).
- **The photo tools send image-derived TEXT, not "just metadata".** `search_photos` (`mcp/executor/photos.rs`) returns
  matched image paths plus the in-image OCR snippet and Vision tags; `image_facts` (`mcp/executor/image_facts.rs`)
  returns the FULL stored OCR text (up to 2,000 characters per file, for up to 200 files) plus tags for paths the caller
  names. A passport scan's OCR text IS the passport number, so this is sensitive derived content, gated by the same
  consent above and named in its copy. Image bytes and thumbnails NEVER egress: both result DTOs are text-only by
  construction (each pinned by a test).
- **Chats and optional call logs stay local.** Conversations live in a local `main.db`; the optional LLM call log writes
  to a local folder and is never transmitted.
