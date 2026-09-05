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
- `file-explorer/rename/`: Inline rename with validation, conflict resolution, extension change, and arrow-key chaining
  across a run of files
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
  `scan-throughput.ts`, the foreground-ownership slots, and the main window's conflict prompt for an operation no
  progress dialog is showing
- `error-messages/`: The user-facing error WORDS (titles, explanations, provider suggestions) for the
  listing/git/empty-root paths, rendered from the typed `ListingError` Rust ships; plus the markdown escaper (the XSS
  boundary). Canonical home of error copy
- `file-operations/transfer/`: Copy + move dialogs, progress dialog (reused by delete/trash), error rendering,
  scan-phase body
- `file-operations/delete/`: F8 / Shift+F8 delete + trash confirmation dialog and pure utilities
- `file-operations/mkdir/`: F7 new-folder dialog with AI folder-name suggestions
- `file-operations/mkfile/`: Shift+F4 new-file dialog
- `file-operations/operation-session/`: The window's event fan-out (seven broadcast write streams demultiplexed per
  `operationId`, with a bounded holding area for operations no session has claimed yet) and the refcounted session
  registry that gives every view of one operation the same derived read state (the smoothed ETA and the scan rates) and
  the same guarded commands (pause, resume, cancel, rollback, answer a clash). The queue rows and the corner chip bind
  to it; so does the main window's conflict prompt. See
  `apps/desktop/src/lib/file-operations/operation-session/CLAUDE.md`
- `file-operations/queue/`: Standalone operation-queue window (per-row pause/resume/cancel/rollback through each row's
  session, multi-select, global pause/resume), rendered from an ops store merging `operations-changed` +
  `write-progress`; route at `routes/queue/`. See `apps/desktop/src/lib/file-operations/queue/CLAUDE.md`
- `file-viewer/`: Read-only file viewer (separate window, virtual scrolling)
- `settings/`: Settings UI + registry-based architecture, reactive state
- `intl/`: The two locale sources (`getUiLocale` for catalog text, `getFormatLocale` for the OS's number/date
  conventions) + memoized locale-aware number/size formatters; counts, file sizes, and the `'system'` date read the
  formatting one (dates formatted in `settings/format-utils.ts`). `os-locales.ts` fetches both from the backend: the
  `'system'` UI language and the composed formatting tag
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
- `crash-reporter/`: Frontend half of the crash pipeline: detects the persisted crash file, picks the dialog sentence
  that's true for it (`crash-copy.ts`), and offers to send it
- `attach-email/`: The "Attach my email address" opt-in shared by the crash-report, error-report, and feedback dialogs:
  reuses the address on file (following it live, with a link into Settings) or collects one. See
  `apps/desktop/src/lib/attach-email/CLAUDE.md`
- `ai/`: Local LLM features (folder suggestions), download flow. Runtime states only; first-launch consent owned by
  `onboarding/`
- `indexing/`: Drive index state, events, priority triggers, scan status overlay
- `status-corner/`: The main window's top-right ambient-status row: owns the corner placement, hosts the
  backgrounded-operation chip and the indexing hourglass. See `apps/desktop/src/lib/status-corner/CLAUDE.md`
- `downloads/`: Go-to-latest action, settings-gated download notifications, global shortcut bridge
- `low-disk-space/`: Low-disk-space warning frontend: event bridge, mode/threshold helpers, Settings deep-link
- `notifications/`: Shared macOS notification permission flow, used by `downloads/` and `low-disk-space/`
- `open-terminal/`: "Open terminal here" frontend: which folder "here" means, where the command is offered, the
  first-use app picker and its two toasts. See `apps/desktop/src/lib/open-terminal/CLAUDE.md`
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
- `dialog-gallery/`: Dev-only inventory of every registered soft dialog, opening each on demand with fixture data for
  design review; the two it can't open say why (Debug > Soft dialogs, rendered over the main window). See
  `apps/desktop/src/lib/dialog-gallery/CLAUDE.md`
- `tooltip/`: Lightweight tooltip primitive
- `stores/`: App-wide reactive Svelte stores: volume list, restricted-paths state
- `updates/`: Auto-updater UI
- `whats-new/`: Post-update "What's new" changelog popup: the pure show/stamp decision, the startup trigger, the soft
  dialog, and the manual Help reopen. See `whats-new/CLAUDE.md`
- `operation-log/`: The alpha "Operation log" dialog (View menu / ⌘⌥L) over the read API — trigger + paging state, the
  soft dialog, the pure typed-enum→i18n label mapping, and the per-row Roll back button that hands a reversal to the
  operation queue. See `apps/desktop/src/lib/operation-log/CLAUDE.md`
- `suggested-ops/`: The "Suggested ops" dialog (View menu) where the user reviews what Ask Cmdr proposed and decides
  what runs, over the proposal spine. Windowed op paging and the disclosure rules: its `CLAUDE.md`
- `utils/`: Filename validation, confirm dialog utilities
- `path/`: Path manipulation helpers (normalize, segment, join/split, platform-aware comparators)
- `font-metrics/`: Character width measurement for accurate Brief mode column sizing

**Adding a new top-level window** (route + opener + capability file, plus the perms gotcha that window-creation is
checked against the calling window): see `guides/adding-a-window.md`. Existing windows are Settings, the File viewer,
and Keyboard shortcuts.

**Frontend text measurement uses `@chenglou/pretext`.** Whenever you need to measure text on the frontend, reach for
pretext (its full API reference is at `apps/desktop/node_modules/@chenglou/pretext/README.md`) rather than a Canvas
`measureText` or DOM-reflow path. For string pixel widths (column shrink-wrapping, middle-truncation, viewer line
heights), call `createPretextMeasure(font, pretext)` from `lib/utils/shorten-middle.ts`. The `font-metrics/` module
above is separate: it ships per-character widths to Rust for backend column sizing.

## Backend (Rust + Tauri 2)

All under `apps/desktop/src-tauri/src/`.

- `file_system/listing/`: Directory reading, streaming, caching, sorting (serves virtual scroll)
- `apps/desktop/src-tauri/src/file_system/staging.rs`: Where every Cmdr scratch file (`.cmdr-tmp-*`, `.cmdr-temp-*`) is
  named, and whether a listing shows it. Rules and rationale: the module's own docs
- `file_system/write_operations/`: Copy/move/delete with safety patterns (temp+rename, staging, rollback). Umbrella +
  shared state machine, the operation manager (queue + lane admission), `OperationEventSink` and the per-source outcome
  it carries, Settle contract, the source-identity binding a reviewed batch may supply (`source_binding.rs`), the
  cross-volume entry routing every transfer goes through (`routing.rs`), and the scan preview a confirmed transfer waits
  on inside its own task (`scan_preview.rs` / `scan_cache.rs` / `scan_bridge.rs`)
- `file_system/write_operations/transfer/`: Copy + move pipelines: conflict resolution, transfer driver, platform copies
  (`copyfile(3)` / `copy_file_range(2)` / chunked)
- `file_system/write_operations/transfer/volume/`: The cross-volume (Local ↔ MTP ↔ SMB ↔ archive) copy/move engine
  behind a facade module
- `file_system/write_operations/delete/`: Delete walker, trash, oracle-aware delete semantics
- `file_system/volume/`: `VolumeManager` plus the `backends/` umbrella, re-exporting the `Volume` trait and its types
  from `crates/cmdr-fs/`. Checklist + capability matrix for new backends
- `file_system/volume/backends/`: the one `Volume` impl that still lives in the app, `LocalPosixVolume`. Every crate
  backend (`cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`, `cmdr-mtp`) is imported by crate name at
  its call sites, and each one's app-side tests sit beside the app code they assert on. What stays app-side is what
  needs the app: archive routing and the archive LRU, SMB's mount and upgrade passes, and edit / transfer driving
- `file_system/git/`: the app's two git seams: the `.git/` listing overlay and the wiring (the parked portal, the
  toggle, the `git-state-changed` event). The repository half is `crates/cmdr-git` hooks, typed git-error classification
  (`FriendlyGitErrorKind`)
- `file_system/terminal.rs`: "Open terminal here": which terminal apps Cmdr knows, how each one takes a folder, and
  whether a pane's folder has a path a shell can reach. Rationale: the module's own docs; the sources behind the table:
  `docs/notes/terminal-launch-sources-2026-09-04.md`
- `file_viewer/`: Three-backend file viewer (FullLoad, ByteSeek, LineIndex)
- `network/`: SMB's app-side half: mDNS discovery, share listing (smb2 + smbutil/smbclient fallback), mounting,
  Keychain, the auto-upgrade passes, and the frontend's connection events. The backend under it is `crates/cmdr-smb/`
- `clipboard/`: File clipboard (Cmd+C/X/V) with NSPasteboard interop; tracks cut state and validates at paste
- `secrets/`: Pluggable secret storage: Keychain (macOS), Secret Service (Linux), encrypted-file fallback. SMB creds +
  AI keys
- `mtp/`: the app-side half of the MTP backend (`crates/cmdr-mtp/`): the USB hotplug task with its enabled gate and
  auto-connect, the `tauri_specta` device events, the registrar and `MtpDeviceProvider` wiring, the macOS ptpcamerad
  workaround, and where the app parks the one connection manager it built. See `mtp/CLAUDE.md`
- `adb/`: the app-side half of the Android-over-ADB backend (`crates/cmdr-adb/`): the `host:track-devices` task, the
  `AdbDeviceProvider`, lazy connect on first navigation, eject (forget the device client-side), and the IPC commands.
  See `adb/CLAUDE.md`
- `device_volumes.rs`: the device-provider seam. A `DeviceVolumeProvider` per device backend (`MtpDeviceProvider`,
  `AdbDeviceProvider`), a registry `volume_listing::complete` folds over, and `notify_devices_changed`, the one hotplug
  push channel (it emits `volumes-changed`). Contract in the module doc; the checklist step in
  `file_system/volume/DETAILS.md` § "Building a new volume"
- `listing_overlays.rs`: the listing-overlay seam, same registration shape. A `ListingOverlay` contributes rows a PANE
  sees that no volume holds, folded in by the listing pipeline; today's one contributor is the git portal's `.git/`
  category rows (`file_system/git/overlay.rs`). Why it must never move into a `Volume`: `file_system/volume/DETAILS.md`
  § "Architecture"
- `mcp/`: MCP server (tools, YAML resources, agent-centric API)
- `ai/`: llama-server lifecycle, model download, inference client
- `analytics/`: Anonymous beta usage analytics: hourly `/heartbeat` sender (true DAU + a PII-free config-shape snapshot
  built by allowlist), tri-state consent gate, dev/CI suppression. PostHog feature events ride the same gate
- `install_id.rs`: Two Rust-owned per-install random ids (`anal_` for analytics, `diag_` for diagnostics) that never
  meet by construction. AppHandle-free accessors, one `install-ids.json`
- `prod_instance.rs`: The one definition of "this process is not a real user's production install" (the env vars every
  dev, E2E, and capture launcher sets). Read by the analytics gate and the updater's update-check gate
- `platform.rs`: Shared platform-identity helpers (`os_version()`), used by crash + error reports and the heartbeat
- `licensing/`: Ed25519 license verification, server validation
- `settings/`: Settings persistence (tauri-plugin-store)
- `priority/`: Who gets the volume, and which of its folders come first. The per-volume signals background work yields
  to (interactive > transfers > indexing): `foreground` activity timestamps + the `transfers` gauge, composed by drive
  indexing's scan pacing and media enrichment's pass gate; plus `roots`, a ranked walk order for a volume walked in
  pieces (last session's tabs, favorites, standard home folders, cloud roots, `$HOME`), answered through the
  `HostPolicy::priority_roots` seam and consumed by the index's phase machine. See its
  `apps/desktop/src-tauri/src/priority/CLAUDE.md`
- `operation_log/`: The durable, cross-volume journal of file mutations — the app's first durable DB
  (`operation-log.db`), the foundation for rollback, indexed name search, and a future undo. Single writer thread, a
  forward-migration ladder (not delete-and-recreate) and retention discipline, interned dir prefixes + per-item rows,
  app-side case folding with no collation (stays `sqlite3`-inspectable). See its
  `apps/desktop/src-tauri/src/operation_log/CLAUDE.md`
- `agent/`: The in-app AI agent, whose first user-facing slice is "Ask Cmdr" (a chat rail). Its `agent/llm/` holds the
  provider-agnostic `AgentLlm` seam over the shipped `genai` client, its deterministic fake, and the typed message-part
  model that keeps opaque provider reasoning state lossless. Its `agent/store/proposals/` holds the durable proposal
  spine (sweeps, reviewable groups, ops, and the claim transaction that binds an approval to what the user saw), with
  `agent/suggested_ops/` as the service over it. Its `agent/wake/` is the proactive half: the pure noticing pipeline
  plus the thread that owns the inbox, the timer, and the wakes it hands off. Its `agent/memory/` is the one place the
  agent writes: a jailed, capped Markdown folder under the app data dir, fed back into every turn's prefix. Its
  `agent/outcomes.rs` closes the loop, recording what the user did with each proposal on the two channels that reach
  them and the agent. Its `agent/tools/` is the in-process toolset the chat dispatches; `agent/tools/read/inspect/` is
  the one tool that reads file contents, through `file_viewer`'s headless seam. See its
  `apps/desktop/src-tauri/src/agent/CLAUDE.md`
- `downloads/`: `notify`-based `~/Downloads` watcher, FDA-gated, browser-rename-aware filter, Cmdr-own-write ignore set
- `search/`: In-memory search index (lazy load, rayon parallel scan, glob/regex) + AI query translation (`search/ai/`)
- `selection/`: Selection dialog backend: recent-selections store + cloud AI translation (`selection/ai/`); the matcher
  itself runs in JS
- `go_to_path/`: "Go to path" backend: pure path resolution + fixed-cap recent-paths store. IPC in
  `commands/go_to_path.rs`
- `recents/`: The persisted recents list all three of those keep (dedupe, cap, durable JSON file, quarantine). A
  consumer supplies the entry type and its dedupe key. See `apps/desktop/src-tauri/src/recents/CLAUDE.md`
- `font_metrics/`: Binary font metrics cache, per-directory width calculation
- `window_state/`: Main-window size/position persistence across launches (`.window-state.json`). Replaces
  `tauri-plugin-window-state`. Placement only; the frontend owns showing the window
- `text_size.rs`: macOS Accessibility text-size watcher (undocumented Apple APIs, risk notes in source). Emits
  `system-text-size-changed`
- `system_strings.rs`: Localized macOS pane labels from `.loctable` system bundles (loctable catalog + risks in source).
  Also the ordered `AppleLanguages` read that `intl/` walks
- `intl/`: What the OS says about language and region. Walks the user's ordered macOS language preferences against the
  catalogs we ship (refusing to cross a script boundary), composes the formatting tag WebKit can't see (the `-u-rg-`
  region override), and answers the frontend over `get_os_locales`. See its `CLAUDE.md`
- `favorites/`: User-editable favorites. Ordered `favorites.json` store (`{ id, path, name }`) backing the volume
  switcher's "Favorites" section. Seed-once-on-absence, dedup-by-path, pure testable core. Read by `get_favorites()` in
  both `volumes/` twins; mutated via `commands/favorites.rs` (which re-emits `volumes-changed`)
- `volumes/`: macOS location/volume discovery + `NSWorkspace` mount/unmount watcher. Distinct from
  `file_system/volume/`. `get_favorites()` reads the `favorites/` store
- `volumes_linux/`: Linux equivalent: location discovery + mount/unmount via `/proc/mounts` and GVFS
- `volume_listing.rs`: the one volume-list pipeline. Aliases whichever discovery module the platform has, bounds it with
  a timeout, appends every device provider's storages (`device_volumes.rs`), and enriches from the registry;
  `commands/volumes.rs` and `volume_broadcast.rs` both publish what it returns. See
  `apps/desktop/src-tauri/src/volumes/CLAUDE.md`
- `space_poller.rs`: Live disk-space polling (per-volume-type intervals) plus the low-disk-space hysteresis warning
- `fda_gate.rs`: Full Disk Access startup gate: blocks TCC reads + `NSWorkspace` icon calls until FDA is decided. See
  the `tauri-apis` rule in `.claude/rules/`
- `instance_lock.rs`: Single-instance guard: one process per data dir, claimed at startup. See
  `tooling/instance-isolation.md` § Instance lock
- `quit/`: The quit gate. Every exit path asks it first (the two Tauri handlers and the MCP `quit` tool); with
  non-instant operations in flight it holds the exit, raises the countdown dialog, and runs the teardown on its own
  deadline. See `apps/desktop/src-tauri/src/quit/CLAUDE.md`, the frontend `apps/desktop/src/lib/quit/CLAUDE.md`, and the
  agent-facing surface in `apps/desktop/src-tauri/src/mcp/executor/quit.rs`
- `app_lifecycle.rs`: The two Tauri builder handlers `lib.rs` names: `on_window_event` (main-window focus, close, and
  destroy; viewer-window teardown) and `on_run_event` (ready, exit requested, exit), plus the shared
  stop-background-services path all three shutdown routes take
- `stubs/`: Linux compilation stubs for macOS-only modules (Docker E2E pipeline)
- `menu/`: Native menu bar: construction, dispatch mapping, accelerator sync, context-aware enable/disable. The Help
  menu carries the "What's new" item (above "Send feedback…")
- `whats_new/`: Parses the embedded `CHANGELOG.md` into the typed model behind the `get_whats_new` IPC that the frontend
  `whats-new/` popup renders. See `whats_new/CLAUDE.md`
- `quick_look/`: macOS-only `QLPreviewPanel` integration (Shift+Space)
- `drag_image_detection.rs`: macOS method swizzle for drag image size detection
- `drag_image_swap.rs`: Rich/transparent drag image swap for self-drags
- `crash_reporter/`: Crash capture (panic hook + signal handler), next-launch detection, in-session delivery of a
  survived panic, whether the app outlived the panic (`survival.rs`), report sending
- `feedback.rs`: Open-beta feedback: text validation + payload assembly + send to `POST /feedback`. IPC in
  `commands/feedback.rs`
- `error_reporter/`: Error reports: bundle build (manifest + redacted log tail), short-ID + R2 upload, debounced
  auto-dispatcher, and the amend path that adds a note to a report that already went out
- `events/`: App-side Tauri payloads for subsystems that emit typed values instead of wire formats (the drive index,
  media index, importance). See `events/CLAUDE.md`
- `updater/`: macOS custom updater: syncs files into the running `.app` in place so FDA survives updates. Other
  platforms use stock Tauri
- `redact/`: Shared PII redactor (path-shape preserving). Used by both crash and error reporters
- `logging/`: Log directory resolver, `KeepSome(N)` post-rotation pruner, `list_recent_log_files`
- `commands/`: Tauri command definitions (IPC entry points)
- `capabilities/`: Per-window Tauri API permissions; update when using new Tauri APIs from a window
- `icons/`: App icons for all platforms + macOS Tahoe Liquid Glass (Assets.car). See its CLAUDE.md for regeneration

## Workspace crates

All under `crates/`, alongside the four apps. `cmdr-fs`, `cmdr-index`, `cmdr-archive`, `cmdr-smb`, `cmdr-sftp`,
`cmdr-webdav`, `cmdr-adb`, `cmdr-mtp`, and `cmdr-git` carry no `tauri` dependency and no reach into the app;
`index-crate-isolation` enforces that against the `cargo metadata` graph, and caps the public surface of `cmdr-index`,
`cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-mtp`, and `cmdr-git` at the numbers their audits landed
on. The two dev CLIs and the vendored fork are ordinary members.

- `crates/cmdr-fs/`: the filesystem vocabulary and host primitives every layer speaks in — the `Volume` trait and its
  data types, `FileEntry`, typed error classification (`ListingError` / `ListingErrorReason` / `ErrorCategory`, errno →
  reason mapping, provider detection over 18 providers), `InMemoryVolume`, File Provider domain detection (the index
  scanner's "is this a domain root?" and the sync badge's "is any ancestor one?"), thread QoS, process-memory readers,
  poison-free locking. The app re-exports all of it from the original paths. See `crates/cmdr-fs/CLAUDE.md`
  - `src/volume/host/`: the seams a storage backend reaches its host through — pane listings, the runtime handle, typed
    connection events, credentials, index notification, settings, user activity, analytics. What a backend crate is
    written against; the app answers them from `apps/desktop/src-tauri/src/volume_host.rs`. See
    `crates/cmdr-fs/src/volume/host/CLAUDE.md`
- `crates/cmdr-archive/`: the archive backend — a `Volume` over a zip / tar / 7z file that physically lives on another
  volume. Browse + extract for every format, plus temp+rename WRITES for zip, over a decoupled `Volume`-free reading
  core (central-directory parse, synthetic tree, streaming decompress, Zip Slip defense) and the shared boundary
  detector its host routes with. The first backend in its own crate, so it's the worked example for the next one. See
  `crates/cmdr-archive/CLAUDE.md`
- `crates/cmdr-sftp/`: everything Cmdr says to an SFTP server. `SftpVolume` over one SSH connection with one SFTP
  channel, on `russh` + `openssh-sftp-client`, plus host-key trust-on-first-use, the four-rung auth ladder, windowed
  read and write paths, and an error policy that puts back the variants SFTP v3 collapses into one catch-all code. The
  app-side halves are the trusted-host-key store, the saved-server list, and the connect wiring
  (`apps/desktop/src-tauri/src/network/sftp_*.rs`) plus the IPC surface (`apps/desktop/src-tauri/src/commands/sftp.rs`);
  the crate itself asks the app nothing directly. What the frontend calls and what each answer means:
  `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend". Its guardrails, the two crate hazards it is shaped
  around, and which side a test lives on: `crates/cmdr-sftp/CLAUDE.md`. Which crate it is built on and why:
  `docs/notes/sftp-crate-evaluation-2026-08-22.md`. Its Docker servers: `apps/desktop/test/sftp-servers/README.md`.
- `crates/cmdr-webdav/`: everything Cmdr says to a WebDAV server (Nextcloud, ownCloud, Synology, Fastmail, a generic
  NAS). `WebdavVolume` over `reqwest` + `quick-xml`: PROPFIND listings, ranged GET reads, staged PUT+MOVE writes so a
  partial upload never wears the user's filename, Basic auth out of the `CredentialStore` seam, TLS trust from the
  system roots (no host keys), and a three-valued connection state with one unattended re-probe before it asks a person.
  No Digest auth, no watcher, no locks. The app-side halves are the saved-server list and the connect wiring
  (`apps/desktop/src-tauri/src/network/webdav_*.rs`) plus the IPC surface
  (`apps/desktop/src-tauri/src/commands/webdav.rs`); reconnect and sign-in ride the backend-neutral commands. What the
  frontend calls and what each answer means: `crates/cmdr-webdav/DETAILS.md` § "Connecting from the frontend". Its
  guardrails and which side a test lives on: `crates/cmdr-webdav/CLAUDE.md`. Its Docker servers:
  `apps/desktop/test/webdav-servers/README.md`. What it still owes: `docs/specs/webdav-backend-follow-ups.md`.
- `crates/cmdr-adb/`: everything Cmdr says to an Android device over ADB. `AdbVolume` per attached device, rooted at the
  device's real `/`, spoken to the ADB server on loopback (the sync service for stat, list, and transfers, `shell,v2`
  for the verbs it lacks, `host:track-devices` for hotplug), with a typed errno-based error policy and a fake ADB server
  for its suites. The device-side twin of `cmdr-sftp`; the app-side half (tracker task, device provider, lazy connect,
  eject, IPC) is `apps/desktop/src-tauri/src/adb/`. Wire contract, `Volume` answers, and gaps:
  `crates/cmdr-adb/DETAILS.md`; guardrails: `crates/cmdr-adb/CLAUDE.md`
- `crates/cmdr-mtp/`: everything Cmdr says to an Android phone or a PTP camera over USB. Device discovery, the
  per-device PTP session layer (path ↔ handle caches, the priority gate, the interrupt-endpoint event loop, and the
  session-reset recovery a screen lock triggers), `MtpVolume` over one storage area, and the fixture-backed virtual
  device the E2E lane and a dev session drive. The app-side half (hotplug policy, the frontend events, the registrar
  wiring, ptpcamerad) is `apps/desktop/src-tauri/src/mtp/`. Where the boundary runs and which side a test lives on:
  `crates/cmdr-mtp/DETAILS.md`; the wire guardrails that keep a dropped future from wedging a phone:
  `crates/cmdr-mtp/CLAUDE.md` and `crates/cmdr-mtp/src/connection/CLAUDE.md`
- `crates/cmdr-git/`: everything Cmdr knows about a git repository. Repo discovery and info for the breadcrumb chip, the
  cached per-entry status walk behind the status column, the per-repo `.git/*` watcher (reporting a typed snapshot
  through a `GitStateSink`, never to a window), and `GitPortalVolume`, the read-only `Volume` that turns `.git`'s
  branches, tags, commits, stash, worktrees, and submodules into browsable trees a copy can stream out of. The app-side
  half (the route, the `.git/` listing overlay, the toggle, the `git-state-changed` event) is
  `apps/desktop/src-tauri/src/file_system/git/`. Where the boundary runs, the capped surface, and every decision:
  `crates/cmdr-git/DETAILS.md`; guardrails: `crates/cmdr-git/CLAUDE.md`
- `crates/cmdr-smb/`: everything Cmdr says to an SMB server. `SmbVolume` over a live smb2 session, with its change
  watcher, reconnect state machine, and refcounted scan-connection pool, plus the protocol layer under it (address
  building, `smb2::Error` classification, the share-listing vocabulary). The second backend in its own crate; what
  belongs on each side of the boundary, and why: `crates/cmdr-smb/CLAUDE.md`
- `crates/cmdr-index/`: the index — everything Cmdr knows about what's on a volume, what's inside its images, and which
  of its folders matter — behind one `Index` handle the host builds and holds. Tauri-free: everything it needs from an
  application arrives through the traits in `host/`, and everything it reports leaves through an `EventSink`. Three
  subsystems inside, each with its own docs. Crate-level rules and the public surface: `crates/cmdr-index/CLAUDE.md`
  - `indexing/`: background drive indexing (SQLite, a guarded parallel walker, FSEvents), recursive directory sizes. A
    volume's FIRST index arrives in phases, in the order its owner cares about, add-only so a quit loses nothing
    (`crates/cmdr-index/src/indexing/lifecycle/phases/CLAUDE.md`); a completed one replays or rescans as before.
    Per-volume registry (one index DB per drive, not just local) with a per-volume freshness model (Fresh/Stale/gray);
    SMB and MTP drives index too and stay live via smb2 `CHANGE_NOTIFY` / PTP events, with an "admittedly stale" model
    on launch and disconnect. See `crates/cmdr-index/src/indexing/CLAUDE.md`
  - `importance/`: Deterministic folder-importance scoring (pure `scorer/`: values-in/score-out `Weights` + explain
    breakdown) that expensive features (agent, media-ML enrichment) consume. A read-consumer of `indexing/`, sibling to
    `search/`, with its own per-volume `importance.db` store, a multi-volume kind-aware scheduler (Local + SMB scored,
    MTP excluded) that recomputes on scan completion (full) and on live listing changes (incremental) — both driven by a
    neutral lifecycle bus in `indexing/` — and the consumable `ImportanceIndex` read API consumers reach it through
    (queryable even for an unmounted volume). See its `crates/cmdr-index/src/importance/CLAUDE.md`
  - `media_index/`: Image-ML enrichment — makes a volume's images searchable by their content (OCR text, Vision scene/
    object tags, image-similarity "find similar" via feature-print embeddings, and natural-language semantic search via
    an on-demand on-device CLIP model — `clip/`, macOS Core ML, a SEPARATE vector space with independent two-part
    staleness). A read-consumer of `indexing/`, ported from `importance/`, with its own per-volume disposable `media.db`
    (path-keyed, FTS5 OCR + tags, structured tags, and a brute-force cosine vector store over feature-print embeddings),
    a scheduler that enriches on the lifecycle-bus scan-completion edge, importance-prioritized (high-importance folders
    first, below the settings slider threshold deferred, with "always index" overrides + a per-folder privacy exclude),
    inference behind a `VisionBackend` seam (real objc2-vision OCR + classify + feature print, a fake for tests),
    deletion-driven GC gated on a completed scan, and the consumable `MediaIndex` read API (OCR/tag search +
    find-similar, offline after unmount). Enriches local volumes plus opt-in network (SMB) volumes conservatively
    (priority-gated via `priority/`, bandwidth-bounded byte-fetch through the app's own smb2 session for Direct volumes
    with an OS-mount fallback; disconnect pauses without losing coverage; MTP never background-sweeps). Off by default.
    See its `crates/cmdr-index/src/media_index/CLAUDE.md` and `specs/later/indexing/media-ml-index-plan.md`
- `crates/index-query/`: developer CLI that queries the index DB with the `platform_case` collation `sqlite3` can't
  supply, plus the three importance measurement binaries. Depends on `cmdr-index` alone. See
  `docs/tooling/index-query.md`
- `crates/operation-log-dump/`: developer CLI that renders `operation-log.db` through the app's own read functions. Its
  own crate because `operation_log` is an app module, so this is the one dev tool that depends on `cmdr`
- `crates/fsevent-stream/`: vendored fork of the FSEvents stream crate (published as `cmdr-fsevent-stream`), giving the
  drive watcher event IDs and `sinceWhen` replay. macOS-only

## Other apps

- `apps/analytics-dashboard/`: Private SvelteKit dashboard on CF Pages. Aggregates Umami, CF Analytics Engine, Paddle,
  PostHog, GitHub metrics
- `apps/api-server/`: Cloudflare Worker + Hono, split into four documented areas — `src/licensing/` (Paddle, keys),
  `src/telemetry/` (crash/heartbeat/download/update-check, error reports, feedback), `src/website/` (beta signup, blog
  likes, `?r=` codes), and `src/admin/` (the dashboard's aggregations)
- `apps/website/`: getcmdr.com marketing site (Astro + Tailwind v4), including the dev-only `/dev/blog` Markdown editor
  for drafting posts
- `apps/website/public/hero/`: Hero illustration assets (frame + pane cutouts, dark/light)
- `apps/desktop/packaging/homebrew/`: Homebrew cask shape source-of-truth and tap-bump flow
- `scripts/check/`: Go unified check runner (~40 checks, parallel with dependency graph)

## Search

Whole-drive file search powered by the index DB. The search index lazy-loads all entries into memory (~600 MB for 5M
files), scans them with rayon in parallel, returns results by recency, and drops after an idle timeout. Backend: a pure
`search()` function (no I/O) in `search/engine.rs`, the compiled query it matches with in `search/matcher.rs`,
DB-touching ops in `query.rs`, global index lifecycle in `index.rs`, AI query translation in `search/ai/`. The dialog is
the first `query-ui` consumer; "Open in pane" hands off to a frontend-only virtual `search-results` volume (snapshot
store, refcounted). Full detail: `src-tauri/src/search/CLAUDE.md` and `apps/desktop/src/lib/search/CLAUDE.md`.

## Cross-cutting patterns

For detailed architecture patterns (data flow, navigation lifecycle, listing lifecycle, concurrency guards,
cancellation, volume mount/unmount, error recovery, persistence), see `architecture-patterns.md`. Read the relevant
section when working on navigation, file operations, or volumes.

### Platform constraints

The Rust backend's cross-cutting filesystem and IPC guardrails (synchronous commands block the IPC handler thread;
network-mount syscalls block 30-120s; the two-layer timeout defense; no rayon for macOS-framework calls) are must-knows
for backend work and live in `apps/desktop/src-tauri/CLAUDE.md` § Platform constraints.

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
  `tooling/instance-isolation.md` for the per-resource breakdown
- `withGlobalTauri: true` in dev mode (security risk if loading remote content)

### Checker script

Go-based unified runner (`scripts/check/`). Parallel execution with dependency graph. Coverage: 70% threshold enforced,
`coverage-allowlist.json` exempts Tauri/DOM-dependent files.

## Diagnostics

Two pipelines report what went wrong on a user's machine, both redacting payloads through the shared `redact/` module
first (see `security.md` for the privacy posture):

- **Crash reporter** (`crash_reporter/`): captures panics + signals, persists a report to disk, and offers to send it on
  the next launch. Targets `POST /crash-report`. A panic the app SURVIVES also goes out in the same session, through the
  error reporter's Flow B; the split is in `apps/desktop/src-tauri/src/crash_reporter/DETAILS.md`.
- **Error reporter** (`error_reporter/` + `error-reporter/`): everything else (MTP weirdness, network glitches, generic
  failures), user-initiated (**Help > Send error report…**) or auto-send opt-in. Bundles a redacted debug-log tail,
  uploads to R2 via `POST /error-report`, returns a short `ERR-XXXXX` ID, and pings a private Discord channel.

Detail in the colocated `CLAUDE.md` files.

## Acquisition analytics / `?r=` tracking

Spans four surfaces. A short `?r=<code>` on a link expands to `utm_source` (+ `utm_medium`) client-side before analytics
runs, so downloads attribute to a channel without a consent banner:

- **api-server** (`apps/api-server/`): KV store for the code map + validation (`src/website/`) and the `/admin/funnel`
  aggregation (`src/admin/`). Serves `GET /r-codes.json` (edge-cached) and `/admin/r-codes` CRUD. See
  `apps/api-server/src/website/CLAUDE.md`.
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

- `tooling/logging.md`: Unified logging, `RUST_LOG` recipes for every subsystem
- `tooling/testing.md`: Testing tools inventory (Rust, Vitest, Playwright, Linux E2E, Docker SMB)
- `tooling/mcp.md`: MCP servers (`cmdr`, `tauri`) for agent-driven app testing
- `tooling/instance-isolation.md`: `CMDR_INSTANCE_ID` primer: per-resource isolation for parallel dev / E2E
- `tooling/css-health-checks.md`: Stylelint + Go-based unused CSS checker
- `tooling/index-query.md`: `index_query`: query index DB with `platform_case` collation (`sqlite3` can't)

The check runner and E2E testing docs live colocated with their code:

- Check runner: `scripts/check/CLAUDE.md`
- Check authoring conventions (write a new check, registry, helpers): `scripts/check/checks/CLAUDE.md`
- E2E overview (all suites, fixtures): `apps/desktop/test/CLAUDE.md`
- Playwright E2E (tauri-playwright, cross-platform): `apps/desktop/test/e2e-playwright/CLAUDE.md`
- Linux E2E (Docker, VNC, legacy): `apps/desktop/test/e2e-linux/CLAUDE.md`

### Dependency management

[Renovate](https://docs.renovatebot.com/) (`renovate.json` in repo root) auto-updates all dependencies (npm, Cargo, Go).
Weekly grouped PRs for non-major updates (auto-merge), monthly for major (manual review). Security vulnerability patches
get immediate auto-merging PRs regardless of schedule. For adding deps by hand, see
[add an npm dependency](guides/add-npm-dependency.md), [add a Rust crate](guides/add-rust-dependency.md), and
[update dependencies](guides/update-dependencies.md).

### External services

- `tooling/hetzner-vps.md`: Production VPS: SSH access, layout, deploy commands
- `tooling/umami.md`: Website analytics: API access, DB queries, troubleshooting
- `tooling/cloudflare.md`: Cmdr zones, workers, Pages, D1 telemetry
- `tooling/posthog.md`: Cmdr project ID and settings
- `tooling/monitoring.md`: UptimeRobot: uptime checks, alerts
- `tooling/analytics-dashboard.md`: Private dashboard at `analdash.getcmdr.com` aggregating metrics for the maintainer
- `tooling/remark42.md`: Self-hosted comments engine for the website (Docker on the Hetzner VPS)
- `tooling/listmonk.md`: Mailing-list manager: the Cmdr beta-tester double-opt-in list and the `/beta-signup` wiring
- `tooling/discord.md`: Community Discord: read-only bot access (tested cURLs) for summarizing channel activity
- `tooling/feedback-and-error-digest.md`: Read in-app feedback (D1) and error-report bundles (R2) straight from the
  source, behind the `/feedback-and-error-digest-from-app` command
- Paddle (payments): `guides/test-purchase-flow.md` walks the sandbox buy-and-activate test end to end

ONLY do read-only operations with these services unless specifically asked to make changes.
