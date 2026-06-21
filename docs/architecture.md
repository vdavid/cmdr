# Architecture

The orientation map: what each subsystem is and where it lives. Per-subsystem must-knows (invariants, gotchas,
guardrails) live in each directory's colocated `CLAUDE.md`; each area's full docs (architecture narrative, flows,
decision detail) live in its colocated `DETAILS.md` (see `AGENTS.md` § File structure for the split contract).
Cross-cutting platform constraints that belong to no single subsystem are in the
[Cross-cutting patterns](#cross-cutting-patterns) section below.

## Frontend (Svelte 5 + TypeScript)

All under `apps/desktop/src/lib/`.

- `file-explorer/`: Dual-pane file explorer: pane orchestration, selection, navigation, sorting. State lives in the
  `explorer-state.svelte.ts` module store
- `file-explorer/views/`: Virtual-scrolling file lists (Brief + Full modes), 100k+ files
- `file-explorer/drag/`: Native drag-and-drop (drag-out, drop-in, pane-to-pane, macOS image swizzle)
- `file-explorer/rename/`: Inline rename with validation, conflict resolution, extension change
- `file-explorer/selection/`: Space/Shift/Cmd selection, range operations
- `file-explorer/navigation/`: Back/forward history, breadcrumb, path utilities
- `file-explorer/network/`: Network browser UI (SMB share browsing, login form)
- `file-explorer/git/`: Git browser frontend: breadcrumb chip, status columns, reactive `RepoInfo` store, git portal
  icons. Git settings in `settings/sections/GitSection.svelte`
- `file-explorer/pane/`: Per-pane orchestration: cursor, scroll, dual-pane coordination. Owns the single `navigate.ts`
  transaction, the persistence subscriber, and the `volume-capabilities.ts` table
- `file-explorer/tabs/`: Tab bar and per-pane tab state
- `file-explorer/operations/`: Pane-scoped operation hooks (delete, refresh, swap) wired into the command registry
- `file-explorer/quick-look/`: Frontend Quick Look (Shift+Space) trigger and keyboard plumbing
- `file-operations/`: Umbrella over `transfer/`, `delete/`, `mkdir/`, `mkfile/` dialogs (shared progress dialog) plus
  `scan-throughput.ts`
- `errors/`: The user-facing error WORDS (titles, explanations, provider suggestions) for the listing/git/empty-root
  paths, rendered from the typed `ListingError` Rust ships; plus the markdown escaper (the XSS boundary). Canonical home
  of error copy
- `file-operations/transfer/`: Copy + move dialogs, progress dialog (reused by delete/trash), error rendering,
  scan-phase body
- `file-operations/delete/`: F8 / Shift+F8 delete + trash confirmation dialog and pure utilities
- `file-operations/mkdir/`: F7 new-folder dialog with AI folder-name suggestions
- `file-operations/mkfile/`: Shift+F4 new-file dialog
- `file-viewer/`: Read-only file viewer (separate window, virtual scrolling)
- `settings/`: Settings UI + registry-based architecture, reactive state
- `intl/`: The single locale source (`getLocale`) + memoized locale-aware number/size formatters; counts, file sizes,
  and the `'system'` date all read it (dates formatted in `settings/format-utils.ts`)
- `shortcuts/`: Keyboard shortcut customization, scope hierarchy, conflict detection, plus the read-only Help > Keyboard
  shortcuts window (`shortcuts-window.ts` + `ShortcutsList.svelte` + pure `shortcut-diff.ts`, route at
  `routes/shortcuts/`)
- `ipc/`: Auto-generated `tauri-specta` bindings (`bindings.ts`). Don't edit by hand; call through `tauri-commands/`
- `tauri-commands/`: Typed TypeScript wrappers around `ipc/bindings.ts`. Canonical import path for backend IPC
- `command-palette/`: Fuzzy command search (~77 palette-visible commands)
- `commands/`: Typed command registry (`CommandId` union + `CommandArgs`), fuzzy search engine. Every entry path
  dispatches through `handleCommandExecute`
- `licensing/`: License validation, commercial reminders, expiration modals
- `logging/`: Unified logging: LogTape config, batching bridge to Rust, verbose toggle
- `error-reporter/`: Error report dialog (Flow A), auto-send toast (Flow B), shared `error-report-flow`
- `feedback/`: Open-beta "Send feedback" dialog (free text + optional reply-to email → api-server). Shared links in
  `lib/beta-links.ts`
- `crash-reporter/`: Frontend half of the crash pipeline: detects the persisted crash file and offers to send it
- `ai/`: Local LLM features (folder suggestions), download flow. Runtime states only; first-launch consent owned by
  `onboarding/`
- `indexing/`: Drive index state, events, priority triggers, scan status overlay
- `downloads/`: Go-to-latest action, settings-gated download notifications, global shortcut bridge
- `low-disk-space/`: Low-disk-space warning frontend: event bridge, mode/threshold helpers, Settings deep-link
- `notifications/`: Shared macOS notification permission flow, used by `downloads/` and `low-disk-space/`
- `go-to-path/`: "Go to path" (⌘G) dialog + handler: thin presenter over backend `resolve_go_to_path`, recents mirror
- `query-ui/`: Shared filter-and-act-on primitives for Search and Selection: `QueryBar`, `ModeChips`, `QueryResults`,
  recent-items, `createQueryFilterState()`
- `query-ui/filter-chips/`: Filter chip popover subsystem (size/modified/scope/pattern)
- `search/`: Whole-drive file search dialog (first `query-ui` consumer): scope, AI label/pattern, snapshot store +
  virtual `search-results` volume, "Open in pane"
- `selection-dialog/`: "Select files…" / "Deselect files…" dialog (second `query-ui` consumer): pure glob/regex +
  size/date matcher, cloud AI translation
- `mtp/`: MTP (Android device) file browsing UI
- `onboarding/`: Soft-sheet onboarding wizard: Full Disk Access, AI provider, open-beta analytics disclosure, optional
  settings
- `ui/`: Shared UI primitives: ModalDialog, Button, AlertDialog, Notification, dialog registry, `SectionCard`
- `routes/(main)/`: The main route: app orchestrator mounting the dual-pane explorer plus top-level dialogs
- `routes/dev/components/`: Dev-only catalog of every `lib/ui/` primitive (Storybook replacement), in the Debug window
- `tooltip/`: Lightweight tooltip primitive
- `stores/`: App-wide reactive Svelte stores: volume list, restricted-paths state
- `updates/`: Auto-updater UI
- `whats-new/`: Post-update "What's new" changelog popup: the pure show/stamp decision, the startup trigger, the soft
  dialog, and the manual Help reopen. See `whats-new/CLAUDE.md`
- `utils/`: Filename validation, confirm dialog utilities
- `path/`: Path manipulation helpers (normalize, segment, join/split, platform-aware comparators)
- `font-metrics/`: Character width measurement for accurate Brief mode column sizing

**Adding a new top-level window** (route + opener + capability file, plus the perms gotcha that window-creation is
checked against the calling window): see [guides/adding-a-window.md](guides/adding-a-window.md). Existing windows are
Settings, the File viewer, and Keyboard shortcuts.

**Frontend text measurement uses `@chenglou/pretext`.** Whenever you need to measure text on the frontend, reach for
pretext (its full API reference is at `apps/desktop/node_modules/@chenglou/pretext/README.md`) rather than a Canvas
`measureText` or DOM-reflow path. For string pixel widths (column shrink-wrapping, middle-truncation, viewer line
heights), call `createPretextMeasure(font, pretext)` from `lib/utils/shorten-middle.ts`. The `font-metrics/` module
above is separate: it ships per-character widths to Rust for backend column sizing.

## Backend (Rust + Tauri 2)

All under `apps/desktop/src-tauri/src/`.

- `file_system/listing/`: Directory reading, streaming, caching, sorting (serves virtual scroll)
- `file_system/write_operations/`: Copy/move/delete with safety patterns (temp+rename, staging, rollback). Umbrella +
  shared state machine, the operation manager (queue + lane admission), `OperationEventSink`, Settle contract
- `file_system/write_operations/transfer/`: Copy + move pipelines: conflict resolution, transfer driver, platform copies
  (`copyfile(3)` / `copy_file_range(2)` / chunked)
- `file_system/write_operations/delete/`: Delete walker, trash, oracle-aware delete semantics
- `file_system/volume/`: `Volume` trait + `VolumeManager`. Umbrella over `backends/` and `friendly_error/`. Checklist +
  capability matrix for new backends
- `file_system/volume/backends/`: Per-backend `Volume` impls: `LocalPosixVolume`, `MtpVolume`, `SmbVolume` (+
  `SmbWatcher`), `InMemoryVolume`
- `file_system/volume/friendly_error/`: typed, word-free error CLASSIFICATION (`ListingError` / `ListingErrorReason`,
  `ErrorCategory`, errno → reason mapping, provider detection over 18 providers). The user-facing WORDS live on the FE
  (`src/lib/errors/`)
- `file_system/git/`: Git browser: repo discovery/info/status, watcher, virtual `.git` portal wired through `Volume`
  hooks, typed git-error classification (`FriendlyGitErrorKind`)
- `file_viewer/`: Three-backend file viewer (FullLoad, ByteSeek, LineIndex)
- `network/`: SMB: mDNS discovery, share listing (smb2 + smbutil/smbclient fallback), mounting, Keychain
- `clipboard/`: File clipboard (Cmd+C/X/V) with NSPasteboard interop; tracks cut state and validates at paste
- `secrets/`: Pluggable secret storage: Keychain (macOS), Secret Service (Linux), encrypted-file fallback. SMB creds +
  AI keys
- `mtp/`: MTP device management, file ops, event-based watching
- `mcp/`: MCP server (tools, YAML resources, agent-centric API)
- `ai/`: llama-server lifecycle, model download, inference client
- `analytics/`: Anonymous beta usage analytics: hourly `/heartbeat` sender (true DAU + a PII-free config-shape snapshot
  built by allowlist), tri-state consent gate, dev/CI suppression. PostHog feature events ride the same gate
- `install_id.rs`: Two Rust-owned per-install random ids (`anal_` for analytics, `diag_` for diagnostics) that never
  meet by construction. AppHandle-free accessors, one `install-ids.json`
- `platform.rs`: Shared platform-identity helpers (`os_version()`), used by crash + error reports and the heartbeat
- `licensing/`: Ed25519 license verification, server validation
- `settings/`: Settings persistence (tauri-plugin-store)
- `indexing/`: Background drive indexing (SQLite, jwalk, FSEvents), recursive directory sizes. Per-volume registry (one
  index DB per drive, not just local) with a per-volume freshness model (Fresh/Stale/gray); SMB and MTP drives index too
  and stay live via smb2 `CHANGE_NOTIFY` / PTP events, with an "admittedly stale" model on launch and disconnect
- `downloads/`: `notify`-based `~/Downloads` watcher, FDA-gated, browser-rename-aware filter, Cmdr-own-write ignore set
- `search/`: In-memory search index (lazy load, rayon parallel scan, glob/regex) + AI query translation (`search/ai/`)
- `selection/`: Selection dialog backend: recent-selections store + cloud AI translation (`selection/ai/`); the matcher
  itself runs in JS
- `go_to_path/`: "Go to path" backend: pure path resolution + fixed-cap recent-paths store. IPC in
  `commands/go_to_path.rs`
- `font_metrics/`: Binary font metrics cache, per-directory width calculation
- `text_size.rs`: macOS Accessibility text-size watcher (undocumented Apple APIs, risk notes in source). Emits
  `system-text-size-changed`
- `system_strings.rs`: Localized macOS pane labels from `.loctable` system bundles (loctable catalog + risks in source)
- `favorites/`: User-editable favorites. Ordered `favorites.json` store (`{ id, path, name }`) backing the volume
  switcher's "Favorites" section. Seed-once-on-absence, dedup-by-path, pure testable core. Read by `get_favorites()` in
  both `volumes/` twins; mutated via `commands/favorites.rs` (which re-emits `volumes-changed`)
- `volumes/`: macOS location/volume discovery + `NSWorkspace` mount/unmount watcher. Distinct from
  `file_system/volume/`. `get_favorites()` reads the `favorites/` store
- `volumes_linux/`: Linux equivalent: location discovery + mount/unmount via `/proc/mounts` and GVFS
- `space_poller.rs`: Live disk-space polling (per-volume-type intervals) plus the low-disk-space hysteresis warning
- `fda_gate.rs`: Full Disk Access startup gate: blocks TCC reads + `NSWorkspace` icon calls until FDA is decided. See
  the `tauri-apis` rule in `.claude/rules/`
- `stubs/`: Linux compilation stubs for macOS-only modules (Docker E2E pipeline)
- `menu/`: Native menu bar: construction, dispatch mapping, accelerator sync, context-aware enable/disable. The Help
  menu carries the "What's new" item (above "Send feedback…")
- `whats_new/`: Parses the embedded `CHANGELOG.md` into the typed model behind the `get_whats_new` IPC that the frontend
  `whats-new/` popup renders. See `whats_new/CLAUDE.md`
- `quick_look/`: macOS-only `QLPreviewPanel` integration (Shift+Space)
- `drag_image_detection.rs`: macOS method swizzle for drag image size detection
- `drag_image_swap.rs`: Rich/transparent drag image swap for self-drags
- `crash_reporter/`: Crash capture (panic hook + signal handler), next-launch detection, report sending
- `feedback.rs`: Open-beta feedback: text validation + payload assembly + send to `POST /feedback`. IPC in
  `commands/feedback.rs`
- `error_reporter/`: Error reports: bundle build (manifest + redacted log tail), short-ID + R2 upload, debounced
  auto-dispatcher
- `updater/`: macOS custom updater: syncs files into the running `.app` in place so FDA survives updates. Other
  platforms use stock Tauri
- `redact/`: Shared PII redactor (path-shape preserving). Used by both crash and error reporters
- `logging/`: Log directory resolver, `KeepSome(N)` post-rotation pruner, `list_recent_log_files`
- `commands/`: Tauri command definitions (IPC entry points)
- `capabilities/`: Per-window Tauri API permissions; update when using new Tauri APIs from a window
- `icons/`: App icons for all platforms + macOS Tahoe Liquid Glass (Assets.car). See its CLAUDE.md for regeneration

## Other apps

- `apps/analytics-dashboard/`: Private SvelteKit dashboard on CF Pages. Aggregates Umami, CF Analytics Engine, Paddle,
  PostHog, GitHub metrics
- `apps/api-server/`: Cloudflare Worker + Hono. Licensing, telemetry, crash reports, downloads, admin endpoints
- `apps/website/`: getcmdr.com marketing site (Astro + Tailwind v4), including the dev-only `/dev/blog` Markdown editor
  for drafting posts
- `apps/website/public/hero/`: Hero illustration assets (frame + pane cutouts, dark/light)
- `apps/desktop/packaging/homebrew/`: Homebrew cask shape source-of-truth and tap-bump flow
- `scripts/check/`: Go unified check runner (~40 checks, parallel with dependency graph)

## Search

Whole-drive file search powered by the index DB. The search index lazy-loads all entries into memory (~600 MB for 5M
files), scans them with rayon in parallel, returns results by recency, and drops after an idle timeout. Backend: a pure
`search()` function (no I/O) in `search/engine.rs`, DB-touching ops in `query.rs`, global index lifecycle in `index.rs`,
AI query translation in `search/ai/`. The dialog is the first `query-ui` consumer; "Open in pane" hands off to a
frontend-only virtual `search-results` volume (snapshot store, refcounted). Full detail:
`src-tauri/src/search/CLAUDE.md` and `apps/desktop/src/lib/search/CLAUDE.md`.

## Cross-cutting patterns

For detailed architecture patterns (data flow, navigation lifecycle, listing lifecycle, concurrency guards,
cancellation, volume mount/unmount, error recovery, persistence), see
[architecture-patterns.md](architecture-patterns.md). Read the relevant section when working on navigation, file
operations, or volumes.

### Platform constraints

The Rust backend's cross-cutting filesystem and IPC guardrails (synchronous commands block the IPC handler thread;
network-mount syscalls block 30-120s; the two-layer timeout defense; no rayon for macOS-framework calls) are must-knows
for backend work and live in [`apps/desktop/src-tauri/CLAUDE.md`](../apps/desktop/src-tauri/CLAUDE.md) § Platform
constraints.

### macOS specifics

- **Full Disk Access**: probed to detect grant state and to register Cmdr in the System Settings list (the two are
  separate jobs, and registration is macOS-version-dependent). The full mechanism lives in one place: the module doc of
  `src-tauri/src/permissions.rs`. Prompt on first launch. See `permissions.rs` and
  `apps/desktop/src/lib/onboarding/CLAUDE.md`.
- **Keychain**: stores network credentials and trial state. Uses `security-framework` crate.
- **copyfile(3)**: preserves xattrs, ACLs, resource forks. `COPYFILE_CLONE` for instant APFS clones.
- **ptpcamerad**: auto-claims USB devices. MTP shows workaround dialog with Terminal command.
- **File Provider integration**: `file_system/cloud_actions.rs` calls `NSFileProviderManager` for evict / download on
  iCloud Drive, Dropbox, Google Drive, OneDrive, Box. `file_system/open_with.rs` uses
  `NSWorkspace.URLsForApplicationsToOpenURL:` for "Open with" candidates. Both APIs descend into `fileproviderd` XPC for
  cloud-stub files, which can blow rayon's 2 MB worker stack, so both modules use dedicated 8 MB-stack OS threads. The
  Services menu (Quick Actions, third-party action extensions) is wired via `PredefinedMenuItem::services` in the `cmdr`
  app menu.

### Dev mode

- `pnpm dev` at repo root for hot-reloading Tauri app
- `pnpm dev --worktree <slug>` for a per-worktree isolated session (separate data dir, ports, Dock label)
- License mock via `CMDR_MOCK_LICENSE=commercial`
- MCP servers bind ephemeral ports on `127.0.0.1`; the actual port lives in `<CMDR_DATA_DIR>/mcp.port` and
  `<CMDR_DATA_DIR>/tauri-mcp.port`. `CMDR_MCP_PORT` still pins the Cmdr MCP server for clients that prefer that. See
  [tooling/instance-isolation.md](tooling/instance-isolation.md) for the per-resource breakdown
- `withGlobalTauri: true` in dev mode (security risk if loading remote content)

### Checker script

Go-based unified runner (`scripts/check/`). Parallel execution with dependency graph. Coverage: 70% threshold enforced,
`coverage-allowlist.json` exempts Tauri/DOM-dependent files.

## Diagnostics

Two pipelines report what went wrong on a user's machine, both redacting payloads through the shared `redact/` module
first (see [docs/security.md](security.md) for the privacy posture):

- **Crash reporter** (`crash_reporter/`): captures panics + signals, persists a report to disk, and offers to send it on
  the next launch. Targets `POST /crash-report`. For unexpected aborts only.
- **Error reporter** (`error_reporter/` + `error-reporter/`): everything else (MTP weirdness, network glitches, generic
  failures), user-initiated (**Help > Send error report…**) or auto-send opt-in. Bundles a redacted debug-log tail,
  uploads to R2 via `POST /error-report`, returns a short `ERR-XXXXX` ID, and pings a private Discord channel.

Detail in the colocated `CLAUDE.md` files.

## Acquisition analytics / `?r=` tracking

Spans four surfaces. A short `?r=<code>` on a link expands to `utm_source` (+ `utm_medium`) client-side before analytics
runs, so downloads attribute to a channel without a consent banner:

- **api-server** (`apps/api-server/`): KV store for the code map + validation + the `/admin/funnel` aggregation. Serves
  `GET /r-codes.json` (edge-cached) and `/admin/r-codes` CRUD. See its `CLAUDE.md` / `DETAILS.md`.
- **website** (`apps/website/`) and the personal **blog** (separate repo, `~/projects-git/vdavid/blog`): client-side
  `?r=` expansion, with the logic mirrored (pure module + an inline copy that must run before deferred analytics). See
  `apps/website/CLAUDE.md` § Analytics.
- **analytics-dashboard** (`apps/analytics-dashboard/`): reads the admin endpoints (`/admin/funnel`, `/admin/r-codes`)
  and is where David edits codes day to day.

**Shared invariant:** the code/UTM charset is the cross-repo attribution contract. The download `ref` sanitizer keeps
`[a-z0-9._:-]`; the link-code/UTM sanitizer keeps `[a-z0-9._-]`. Every surface's sanitizer must normalize identically,
or a stored value and a pass-through value diverge and attribution corrupts. The api-server is the source of truth and
re-sanitizes; clients sanitize to reject bad input before a round-trip.

## Tooling and infrastructure

Dev workflow docs and external service references. All in `docs/tooling/`.

### Dev workflow

- [logging.md](tooling/logging.md): Unified logging, `RUST_LOG` recipes for every subsystem
- [testing.md](tooling/testing.md): Testing tools inventory (Rust, Vitest, Playwright, Linux E2E, Docker SMB)
- [mcp.md](tooling/mcp.md): MCP servers (`cmdr`, `tauri`) for agent-driven app testing
- [instance-isolation.md](tooling/instance-isolation.md): `CMDR_INSTANCE_ID` primer: per-resource isolation for parallel
  dev / E2E
- [css-health-checks.md](tooling/css-health-checks.md): Stylelint + Go-based unused CSS checker
- [index-query.md](tooling/index-query.md): `index_query`: query index DB with `platform_case` collation (`sqlite3`
  can't)

The check runner and E2E testing docs live colocated with their code:

- Check runner: [`scripts/check/CLAUDE.md`](../scripts/check/CLAUDE.md)
- Check authoring conventions (write a new check, registry, helpers):
  [`scripts/check/checks/CLAUDE.md`](../scripts/check/checks/CLAUDE.md)
- E2E overview (all suites, fixtures): [`apps/desktop/test/CLAUDE.md`](../apps/desktop/test/CLAUDE.md)
- Playwright E2E (tauri-playwright, cross-platform):
  [`apps/desktop/test/e2e-playwright/CLAUDE.md`](../apps/desktop/test/e2e-playwright/CLAUDE.md)
- Linux E2E (Docker, VNC, legacy): [`apps/desktop/test/e2e-linux/CLAUDE.md`](../apps/desktop/test/e2e-linux/CLAUDE.md)

### Dependency management

[Renovate](https://docs.renovatebot.com/) (`renovate.json` in repo root) auto-updates all dependencies (npm, Cargo, Go).
Weekly grouped PRs for non-major updates (auto-merge), monthly for major (manual review). Security vulnerability patches
get immediate auto-merging PRs regardless of schedule. For adding deps by hand, see
[add an npm dependency](guides/add-npm-dependency.md), [add a Rust crate](guides/add-rust-dependency.md), and
[update dependencies](guides/update-dependencies.md).

### External services

- [hetzner-vps.md](tooling/hetzner-vps.md): Production VPS: SSH access, layout, deploy commands
- [umami.md](tooling/umami.md): Website analytics: API access, DB queries, troubleshooting
- [cloudflare.md](tooling/cloudflare.md): Cmdr zones, workers, Pages, D1 telemetry
- [posthog.md](tooling/posthog.md): Cmdr project ID and settings
- [monitoring.md](tooling/monitoring.md): UptimeRobot: uptime checks, alerts
- [analytics-dashboard.md](tooling/analytics-dashboard.md): Private dashboard at `analdash.getcmdr.com` aggregating
  metrics for the maintainer
- [remark42.md](tooling/remark42.md): Self-hosted comments engine for the website (Docker on the Hetzner VPS)
- [listmonk.md](tooling/listmonk.md): Mailing-list manager: the Cmdr beta-tester double-opt-in list and the
  `/beta-signup` wiring
- [discord.md](tooling/discord.md): Community Discord: read-only bot access (tested cURLs) for summarizing channel
  activity
- [feedback-and-error-digest.md](tooling/feedback-and-error-digest.md): Read in-app feedback (D1) and error-report
  bundles (R2) straight from the source, behind the `/feedback-and-error-digest-from-app` command
- Paddle (payments): [test-purchase-flow.md](guides/test-purchase-flow.md) walks the sandbox buy-and-activate test end
  to end

ONLY do read-only operations with these services unless specifically asked to make changes.
