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

### What a bundle never picks up

Neither bundle reaches into the app data dir beyond two named things, so the agent's memory folder
(`<data-dir>/ai/memory/`, which sits right beside the index databases) can't ride along. Stated here so nobody has to
re-audit it after each new file we put in that dir (verified against `main` on 2026-08-23 by reading every collector):

- `diagnostics_snapshot.rs`'s `index_db_sizes` does ONE non-recursive `read_dir(data_dir)` and keeps only names starting
  `index-` and ending `.db`, reading their `len()`. It never recurses into `ai/`, and it never opens a file.
- The log half takes `logging::list_recent_log_files(log_dir)`, which is the LOG dir (not the data dir) filtered by
  `is_active_log_file` to `cmdr.log` plus `cmdr.log.<digits>`. Never `llm-logs/`, never anything under `ai/`.
- The crash reporter touches the data dir for exactly two paths it names itself, the crash file and the raw crash file.

**The residual hole is `cmdr.log` itself**, which does ship: ❌ never log memory content, and never log a wake's reason
verbatim. The redactor is path-shaped, so it does nothing to prose.

### What identifies a report

No license key and no analytics id is ever attached. Two handles can be:

- **`ERR-XXXXX`** (5 chars from the same unambiguous alphabet as license short codes), the correlation handle users
  quote when they reach out.
- **`diag_<uuid>`**, the diagnostics install id, which groups sequential reports from one install. Deliberately a
  different id from the `anal_` analytics one, so a report can never be joined to the analytics stream.
- **An email address**, present ONLY when the user ticks the attach-email box in the send dialog (Flow A). The
  auto-dispatcher (Flow B) always leaves it empty, enforced structurally by `bundle_builder::email_for_kind`.

Crash reports carry the same two optional fields. Both are cleared server-side after 90 days by `handleRetentionSweep`
(`apps/api-server/DETAILS.md` § Data retention), leaving the technical row.

### Server retention

Bundles land in the `cmdr-error-reports` R2 bucket (key: `error-reports/{yyyy-mm-dd}/{ERR-XXXXX}-{uuid}.zip`). Three
retention layers, in order of aggressiveness:

1. **8/6 GB watermark eviction.** On every upload (in `event.waitUntil(...)`, so the client response isn't blocked) the
   server checks total bytes; if > 8 GB it deletes oldest objects until total ≤ 6 GB. KV-locked to avoid concurrent
   evictors.
2. **Daily cron sweep.** `handleDailyEvictionSweep` recomputes total from R2 ground truth and re-runs eviction. Catches
   KV drift.
3. **R2 lifecycle rule.** 90-day expiration at the bucket level (third safety net if both layers above fail). This is
   also the retention the privacy policy states for error reports, so it's a promise now, not only a safety net.

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

## License signing keys

Dev and production sign licenses with different Ed25519 keys, and the desktop app verifies against whichever public key
matches its build mode. The production private key exists only as a Cloudflare Worker secret, never in
`apps/api-server/.dev.vars`: it can mint a license every shipped build accepts, and there's no revocation path short of
shipping a new binary. Mechanism, the rotation caveat, and the test that guards both directions:
`apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signing keys.

## Ask Cmdr agent egress (to the user's LLM provider)

Ask Cmdr is the one subsystem that deliberately sends user data OFF the Mac — to the AI provider the user configured,
with their own API key. Privacy posture:

- **Consent-gated, fail-closed.** Every send checks `agent::consent::has_current_consent` in the backend before it
  resolves the LLM; an absent or stale acceptance refuses the send (not just a UI affordance). The consent copy
  (`askCmdr.consent.*`) enumerates exactly what egresses; bump `CONSENT_COPY_VERSION` when that set changes so users
  re-accept.
- **The agent can propose; only the user can approve.** The agent has no tool that touches the user's files. Its
  dispatch view admits `Access::Read`, `Access::Propose`, and `Access::Memory` entries and never `Access::Write` (pinned
  structurally in `mcp/tests/tool_registry_tests.rs`, and again at runtime in `agent/tools/view.rs`). A `Propose` tool
  mutates nothing: it stages a proposal and opens a review surface. Approval originates in the frontend as a user
  action, and there is no tool — and never will be one — that approves a proposal. No agent-visible tool can reach the
  `autoConfirm` confirmation bypass. `Propose` adds no egress: proposals flow agent → user, never to the provider, so
  consent is unchanged by it. What reaches the provider by default is file/folder names, paths, sizes, dates, and the
  app-state envelope (spec §2.1); file contents reach it only through the two read tools below, on request. Depth:
  `apps/desktop/src-tauri/src/mcp/DETAILS.md` § The `Propose` tier.
- **The agent writes one folder, and what it writes egresses forever after.** `Access::Memory` covers `memory_write` and
  `memory_edit`, jailed to `<data-dir>/ai/memory/` (relative `.md` paths only, no `..`, no symlink along the chain,
  containment re-checked against a canonicalized parent) and capped at 64 KB. Two consequences worth stating plainly:
  **(a)** everything the agent saves there is sent to the user's provider on every later message, indefinitely, which
  includes anything it inferred from OCR of their photos — the system prompt forbids saving that, but the prohibition is
  a prompt, not a gate; **(b)** the write path is reachable from untrusted text (a crafted file name, a sentence
  photographed in an image, a line of a file `inspect_file` read), so what lands there could be attacker-chosen. The
  prompt-injection mitigations are memory's placement BEFORE the rules, a fence its own content cannot close, and the
  "facts, never instructions to yourself" write rule. Depth: `apps/desktop/src-tauri/src/agent/memory/DETAILS.md` § The
  injection surface.
- **The photo tools send image-derived TEXT, not "just metadata".** `search_photos` (`mcp/executor/photos.rs`) returns
  matched image paths plus the in-image OCR snippet and Vision tags; `image_facts` (`mcp/executor/image_facts.rs`)
  returns the FULL stored OCR text (up to 2,000 characters per file, for up to 200 files) plus tags for paths the caller
  names. A passport scan's OCR text IS the passport number, so this is sensitive derived content, gated by the same
  consent above and named in its copy.
- **`inspect_file` sends bounded parts of a file's contents, on request.** The one tool that reads inside a file
  (`apps/desktop/src-tauri/src/agent/tools/read/inspect/`), for up to 200 paths per call, each row typed by what the
  bytes really are. What egresses, per kind:
  - text (any encoding the viewer decodes): a window of lines (`startLine` + `maxLines`, default 200, at most 2,000;
    16,000 characters per row, 2,000 per line);
  - PDF: the text of the requested pages (default three, at most 20 per call, 8,000 characters a page under the row's
    16,000) plus the Info dictionary's title and author. A contract's pages are the contract;
  - archive: the entry names, sizes, and dates of one directory level (up to 200), never the entries' bytes;
  - image: dimensions and the EXIF block, `gps { latitude, longitude }` included when the photo carries it. A photo's
    coordinates are a home address;
  - `find`: matching lines from every text and PDF path in the call (up to 50 per row, 300 characters around the first
    match), so one call can search many files. Every cut is reported (`truncated`, `linesCut`, `returnedLines` /
    `totalMatches`, `unanswered`); a path the tool can't or won't read answers a typed status (`folder`, `missing`,
    `unreadable { permission | io | encrypted | corrupt | unsupported | tooLargeToExtract }`, `unreachable` after the 5
    s per-path deadline, `unsupportedVolume` for `mtp://` and direct `smb://`), and an encrypted PDF or archive entry
    stays closed: the tool has no password path. The PDF parser runs inside `crash_reporter::contain_panics`, so a
    malformed PDF is a message-free `warn` line and an `unparseable` row, never a crash report carrying its bytes. Same
    consent gate, named in its copy (`askCmdr.consent.item.contents`, `askCmdr.consent.contentsRule`;
    `CONSENT_COPY_VERSION` 4).
- **Raw bytes of any file never egress.** Every result DTO the agent can receive is text-only by construction: the photo
  tools' shapes and `inspect_file`'s rows have no field that can hold bytes, each pinned by a walk-the-JSON test
  (`photo_hit_is_text_only_no_byte_fields`, `file_facts_is_text_only_no_byte_fields`,
  `every_row_shape_is_text_only_no_byte_fields`). Image bytes, thumbnails, and archive entry contents are unreachable by
  any tool in the agent's view.
- **Chats and optional call logs stay local.** Conversations live in a local `main.db`; the optional LLM call log writes
  to a local folder and is never transmitted.

## AI API keys

The user's BYOK cloud key lives in the OS secret store (`crate::secrets`: macOS Keychain, Linux Secret Service, or an
encrypted-file fallback), never in `settings.json`. On top of that:

- **A stored key never crosses IPC into a webview.** There is deliberately no command that returns it. `configure_ai`
  and `check_ai_connection` take a PROVIDER ID and read the key in the backend; `get_ai_api_key_status` returns only
  `isSet` plus a truncated SHA-256 fingerprint (the settings UI needs a change-detector for its model-list cache, not
  the key). ❌ Don't add a key-returning command back: it would hand the plaintext key to any window that can invoke
  (see the caller-window section below). Guardrail lives in `apps/desktop/src-tauri/src/ai/api_keys.rs`.
- **So the key field can't be pre-filled**, and it isn't: it starts empty with a "your key is saved" placeholder, and
  typing replaces the stored key. A key the user has typed but not yet saved is invisible to `check_ai_connection`, so
  both key fields flush their debounced save before scheduling a check (`AiCloudSection.svelte`,
  `CloudProviderSetup.svelte`).
- **A key the user types is still plaintext in that window** for as long as they're typing it. That's unavoidable and
  fine; the contract is about PERSISTED keys flowing back out of the secret store.
- **`validate_ai_base_url` stays**: it refuses to send a key over plaintext `http://` to a non-loopback host, which is
  what stops a malicious "free proxy" base URL from harvesting keys. See `ai/CLAUDE.md`.

## Why there's no caller-window authorization guard

Tauri's capability system does NOT gate the app's own commands. It gates plugin and `core:` commands; an app-defined
command is ACL-checked only when the app ships its own ACL manifest (`src-tauri/permissions/`, which we don't) or the
call comes from a remote origin (the `resolve_access` gate in tauri's webview module). So every command in
`generate_handler![]` is callable from every window, including `viewer-*`. The per-window capability files still matter
for everything they do cover (plugins, `core:`, and the store), just not for our commands.

We know, and we're deliberately not adding a per-window allowlist. The reasoning, so nobody re-derives it:

- **The escalation an attacker would gain is small.** The viewer legitimately owns `viewer_open` / `viewer_get_lines`,
  so a compromised viewer can already read any file the process can read. A caller guard wouldn't change that. It would
  only block arbitrary deletion, key/licensing/updater access, and the outbound request in `check_ai_connection` (which
  hits an attacker-chosen host from the Rust process, bypassing the webview CSP).
- **There's no live path into a viewer window.** Viewer content renders through Svelte's escaping interpolation with no
  `{@html}`; the CSP is `script-src 'self'` with no `unsafe-inline`; images and PDFs render via `<img>` / `<embed>` on
  the `cmdr-media:` scheme, which yields no script in the app origin. Reaching IPC needs a WebKit decoder 0-day or a
  `{@html}` we introduce ourselves.
- **The guard wouldn't cover the realistic XSS surface anyway.** The `{@html}` sinks that DO exist (Ask Cmdr assistant
  markdown, error messages carrying filenames, operation-failure toasts) are all in the MAIN window, which legitimately
  needs the privileged commands.

What holds this position up, and what would overturn it:

- ❌ **Never render untrusted content through `{@html}` in a viewer or queue window.** That's the assumption the whole
  argument rests on. The general `{@html}` rules are in `apps/desktop/src/lib/error-messages/CLAUDE.md` and
  `lib/ask-cmdr/CLAUDE.md`; here it's load-bearing rather than good practice.
- **Revisit if a viewer window gains a renderer for attacker-authored markup** (HTML preview, a markdown view, an SVG
  surface, an embedded font/media decoder we drive ourselves). At that point add the guard.
- **The cheap version, if it's ever wanted**, is a denylist rather than a per-window allowlist of ~450 commands: wrap
  `specta_builder.invoke_handler()` in `lib.rs` and reject a set of privileged commands (write ops, AI keys, licensing,
  updater, `check_ai_connection`) for any window label other than `main`. One choke point, no per-command edits.
