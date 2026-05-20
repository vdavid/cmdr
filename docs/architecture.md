# Architecture

Map of Cmdr's major subsystems. Each directory has detailed docs in their `CLAUDE.md` file!

## Frontend (Svelte 5 + TypeScript)

All under `apps/desktop/src/lib/`.

| Directory                   | Purpose                                                                                                                                                                                                                                                                                                               |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `file-explorer/`            | Dual-pane file explorer: pane orchestration, selection, navigation, sorting                                                                                                                                                                                                                                           |
| `file-explorer/views/`      | Virtual-scrolling file lists (Brief + Full modes), 100k+ file support                                                                                                                                                                                                                                                 |
| `file-explorer/drag/`       | Native drag-and-drop (drag-out, drop-in, pane-to-pane, macOS image swizzle)                                                                                                                                                                                                                                           |
| `file-explorer/rename/`     | Inline rename with validation, conflict resolution, extension change                                                                                                                                                                                                                                                  |
| `file-explorer/selection/`  | Space/Shift/Cmd selection, range operations                                                                                                                                                                                                                                                                           |
| `file-explorer/navigation/` | Back/forward history, breadcrumb, path utilities                                                                                                                                                                                                                                                                      |
| `file-explorer/network/`    | Network browser UI (SMB share browsing, login form)                                                                                                                                                                                                                                                                   |
| `file-explorer/git/`        | Git browser frontend (complete). Breadcrumb chip, status-column helpers, reactive `RepoInfo` store, Lucide-rendered git portal icons via `selection/FileIcon.svelte`. **General > Git** settings section lives in `settings/sections/GitSection.svelte` and can live-disable the backend portal for raw `.git` access |
| `file-operations/`          | Transfer dialogs (copy/move/mkdir) with progress and conflict resolution                                                                                                                                                                                                                                              |
| `file-viewer/`              | Read-only file viewer (opens in separate window, virtual scrolling)                                                                                                                                                                                                                                                   |
| `settings/`                 | Settings UI + registry-based architecture, reactive state                                                                                                                                                                                                                                                             |
| `shortcuts/`                | Keyboard shortcut customization, scope hierarchy, conflict detection                                                                                                                                                                                                                                                  |
| `tauri-commands/`           | Typed TypeScript wrappers for all Tauri IPC commands and events                                                                                                                                                                                                                                                       |
| `command-palette/`          | Fuzzy command search (~45 commands)                                                                                                                                                                                                                                                                                   |
| `commands/`                 | Command registry (~50 commands), fuzzy search engine for command palette                                                                                                                                                                                                                                              |
| `licensing/`                | License validation, commercial reminders, expiration modals                                                                                                                                                                                                                                                           |
| `logging/`                  | Unified logging: LogTape config, batching bridge to Rust, verbose toggle                                                                                                                                                                                                                                              |
| `error-reporter/`           | Error report dialog (Flow A preview), auto-send toast (Flow B), shared `error-report-flow` entry point                                                                                                                                                                                                                |
| `ai/`                       | Local LLM features (folder suggestions), download flow                                                                                                                                                                                                                                                                |
| `indexing/`                 | Drive index state, events, priority triggers, scan status overlay                                                                                                                                                                                                                                                     |
| `search/`                   | Whole-drive file search dialog: orchestrator + `AiSearchRow`, `SearchInputArea`, `SearchResults` components                                                                                                                                                                                                           |
| `mtp/`                      | MTP (Android device) file browsing UI                                                                                                                                                                                                                                                                                 |
| `onboarding/`               | Full Disk Access prompt for first-launch onboarding                                                                                                                                                                                                                                                                   |
| `ui/`                       | Shared UI primitives: ModalDialog, Button, AlertDialog, LoadingIcon, Notification, dialog registry                                                                                                                                                                                                                    |
| `updates/`                  | Auto-updater UI                                                                                                                                                                                                                                                                                                       |
| `utils/`                    | Filename validation, confirm dialog utilities                                                                                                                                                                                                                                                                         |
| `font-metrics/`             | Character width measurement for accurate Brief mode column sizing                                                                                                                                                                                                                                                     |

**Frontend text measurement always uses `@chenglou/pretext`.** When the frontend needs to know the pixel width of a
string (column shrink-wrapping, middle-truncation, viewer line heights, etc.), call
`createPretextMeasure(font, pretext)` from `lib/utils/shorten-middle.ts` rather than rolling a Canvas `measureText` or
DOM-reflow path. Pretext matches the browser's own text shaping and is dynamically imported so it doesn't bloat the
initial bundle. The separate `font-metrics/` module above is a distinct concern: it ships per-character widths to Rust
for backend column sizing.

## Backend (Rust + Tauri 2)

All under `apps/desktop/src-tauri/src/`.

| Directory/file                  | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `file_system/listing/`          | Directory reading, streaming, caching, sorting (serves virtual scroll)                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `file_system/write_operations/` | Copy/move/delete with safety patterns (temp+rename, staging, rollback)                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `file_system/volume/`           | `Volume` trait + implementations (Local, MTP, SMB, InMemory). Has a checklist and capability matrix for adding new backends; start there                                                                                                                                                                                                                                                                                                                                                                                                       |
| `file_system/git/`              | Git browser (complete). Repo discovery + info + status, watcher, friendly errors. Virtual `.git` portal (`branches/`, `tags/`, `commits/`, `stash/`, `worktrees/`, `submodules/`, `raw/`) wired through the `Volume` hooks so cross-volume copy plucks files from refs for free. `redirect_to_path` pivots worktree/submodule entries to their working dirs. M4 adds a live-toggleable portal hook (`fileExplorer.git.showVirtualGitPortal`) and FriendlyError integration end-to-end via a sentinel-encoded payload on `VolumeError::IoError` |
| `file_viewer/`                  | Three-backend file viewer (FullLoad, ByteSeek, LineIndex)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `network/`                      | SMB: mDNS discovery, share listing (smb-rs + smbutil), mounting, Keychain                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `mtp/`                          | MTP device management, file ops, event-based watching                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `mcp/`                          | MCP server (19 tools, YAML resources, agent-centric API)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `ai/`                           | llama-server lifecycle, model download, inference client                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `licensing/`                    | Ed25519 license verification, server validation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `settings/`                     | Settings persistence (tauri-plugin-store)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `indexing/`                     | Background drive indexing (SQLite, jwalk, FSEvents), recursive directory sizes                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `search/`                       | In-memory search index (lazy load, rayon parallel scan, glob/regex) and AI query translation pipeline (`search/ai/`)                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `font_metrics/`                 | Binary font metrics cache, per-directory width calculation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `text_size.rs`                  | macOS Accessibility text-size watcher. Reads `UIPreferredContentSizeCategoryName` from `NSGlobalDomain`, observes `com.apple.accessibility.api` distributed notifications, emits `system-text-size-changed` to the frontend. Both the key and the notification are undocumented Apple APIs (see source for risk notes). Frontend compounds this with `appearance.textSize` in `lib/text-size.ts`.                                                                                                                                              |
| `system_strings.rs`             | Localized macOS pane labels ("Full Disk Access", "Privacy & Security", "System Settings", ...) loaded from `.loctable` files in system bundles. Picks the user's preferred language from `NSUserDefaults.AppleLanguages` (independent of the app's UI language) and falls back to English on misses. Backend friendly-error builders call `expand("... {full_disk_access} ...")`; frontend caches the snapshot via `lib/system-strings.svelte.ts`. See module-level docs in the file for the loctable catalog and risks.                       |
| `volumes/`                      | Volume abstraction (local, network, MTP), scanner/watcher traits                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `space_poller.rs`               | Live disk-space polling: per-volume-type intervals via `Volume::space_poll_interval()`, threshold-based change detection, emits `volume-space-changed`                                                                                                                                                                                                                                                                                                                                                                                         |
| `stubs/`                        | Linux compilation stubs for macOS-only modules (used by Docker E2E pipeline)                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `menu/`                         | Native menu bar: platform-specific construction, dispatch mapping, accelerator sync, context-aware enable/disable                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `quick_look/`                   | macOS-only `QLPreviewPanel` integration (Shift+Space). Singleton controller behind `Mutex`, `QLPreviewPanelDataSource` + `QLPreviewPanelDelegate` via `define_class!`, key-forward + close events                                                                                                                                                                                                                                                                                                                                              |
| `drag_image_detection.rs`       | macOS method swizzle for drag image size detection                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `drag_image_swap.rs`            | Rich/transparent drag image swap for self-drags                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `crash_reporter/`               | Crash capture (panic hook + signal handler), next-launch detection, report sending                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `error_reporter/`               | Error reports: bundle build (manifest + redacted log tail), short-ID + R2 upload, debounced auto-dispatcher for Flow B                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `redact/`                       | Shared PII redactor (path-shape preserving). Used by both crash and error reporters                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `logging/`                      | Log directory resolver, `KeepSome(N)` post-rotation pruner, `list_recent_log_files` helper used by the error reporter bundle builder                                                                                                                                                                                                                                                                                                                                                                                                           |
| `commands/`                     | Tauri command definitions (IPC entry points)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `capabilities/`                 | Per-window Tauri API permissions; must be updated when using new Tauri APIs from a window                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `icons/`                        | App icons for all platforms + macOS Tahoe Liquid Glass (Assets.car). See [CLAUDE.md](../apps/desktop/src-tauri/icons/CLAUDE.md) for regeneration steps                                                                                                                                                                                                                                                                                                                                                                                         |

## Other apps

| Directory                   | Purpose                                                                                                                                                                                                                          |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apps/analytics-dashboard/` | Private SvelteKit dashboard on CF Pages. Aggregates metrics from Umami, CF Analytics Engine, Paddle, PostHog, GitHub. See [CLAUDE.md](../apps/analytics-dashboard/CLAUDE.md)                                                     |
| `apps/api-server/`          | Cloudflare Worker + Hono. Licensing, telemetry, crash reports, downloads, and admin endpoints. See [CLAUDE.md](../apps/api-server/CLAUDE.md) (technical reference) and [README](../apps/api-server/README.md) (first-time setup) |
| `apps/website/`             | getcmdr.com marketing site (Astro + Tailwind v4). See [README](../apps/website/README.md) and [CLAUDE.md](../apps/website/CLAUDE.md)                                                                                             |
| `apps/website/public/hero/` | Hero illustration assets (frame + pane cutouts, dark/light). See [CLAUDE.md](../apps/website/public/hero/CLAUDE.md) for reshoot process                                                                                          |
| `scripts/check/`            | Go unified check runner (~40 checks, parallel with dependency graph)                                                                                                                                                             |

## Search

Whole-drive file search powered by the index DB. The search index loads all entries into memory (~600 MB for 5M files),
scans them with rayon in parallel, and returns results sorted by recency. The index is loaded lazily when the search
dialog opens and dropped after idle timeout.

**Backend** (`search/`): `engine.rs` has a pure `search()` function (no I/O) that accepts `&SearchIndex` +
`&SearchQuery` and returns `SearchResult`. `types.rs` defines data structures, `query.rs` handles DB-touching operations
(scope resolution, directory sizes), `index.rs` manages global index state with idle/backstop timers. `search/ai/`
contains the AI query translation pipeline (prompt, parser, query builder). See `src-tauri/src/search/CLAUDE.md`.

**IPC** (`commands/search.rs`): Thin wrappers. `prepare_search_index` (starts async load, emits `search-index-ready`
event), `search_files` (returns empty if not loaded), `release_search_index` (starts 5-min idle timer),
`translate_search_query` (orchestrates `search::ai` pipeline). `resolve_ai_backend` handles AI provider config.

**Lifecycle**: Load on dialog open (2-3s for 5M rows with cancellation check every 100K rows) -> search while loaded ->
idle timeout (5 min) or backstop timeout (10 min) drops the index.

## Cross-cutting patterns

For detailed architecture patterns (data flow, navigation lifecycle, listing lifecycle, concurrency guards,
cancellation, volume mount/unmount, error recovery, persistence), see
[architecture-patterns.md](architecture-patterns.md). Read the relevant section when working on navigation, file
operations, or volumes.

### Platform constraints

Rules that cut across many modules. All existing commands follow these; apply them to new code too.

1. **Tauri IPC threading.** Synchronous `#[tauri::command]` functions block the IPC handler thread. If one command hangs
   (e.g., a filesystem syscall on a dead network mount), ALL subsequent IPC calls from the frontend queue behind it and
   the app appears frozen. All filesystem-touching commands are `async` with `blocking_with_timeout` (2s default). When
   adding new commands that touch the filesystem, follow this pattern; see `commands/file_system/` for examples.

2. **Network mount blocking syscalls.** `statfs`, `readdir`, `metadata()`, NSURL resource queries, and `realpath` can
   all block indefinitely on slow/hung network mounts (kernel waits 30–120s). Every Tauri command that calls these is
   wrapped in `blocking_with_timeout`. New commands MUST do the same. See `commands/CLAUDE.md` for the full pattern and
   timeout tiers.

3. **Two-layer timeout defense.** Backend: `blocking_with_timeout` (2–15s) wraps syscalls in `tokio::time::timeout`.
   Frontend: `withTimeout` (500ms–3s) races IPC calls and returns a fallback on expiry. Both layers are applied for
   critical paths (volume switching, path resolution, volume space queries). Apply both when adding new IPC calls to
   slow paths.

### macOS specifics

- **Full Disk Access**: checked by trying to read 1 byte from a list of TCC-protected files
  (`~/Library/Safari/History.db`, `~/Library/Mail/V10/MailData/Envelope Index`, etc.) until one returns either `Ok` (FDA
  granted) or `PermissionDenied` (denied; bundle gets registered with TCC). On denial, also fires `mmap` +
  `NSData dataWithContentsOfFile:` + `read_dir` of the parent (multi-trigger fallback because macOS 26 (Tahoe) can
  short-circuit `read()` denials without consulting tccd). Prompt on first launch. See
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
- License mock via `CMDR_MOCK_LICENSE=commercial`
- MCP server available at `localhost:19224` (prod) / `localhost:19225` (dev) for agent testing
- `withGlobalTauri: true` in dev mode (security risk if loading remote content)

### Checker script

Go-based unified runner (`scripts/check/`). Parallel execution with dependency graph. Coverage: 70% threshold enforced,
`coverage-allowlist.json` exempts Tauri/DOM-dependent files.

## Diagnostics

Two parallel pipelines feed the maintainer with what went wrong on a user's machine. Both pass payloads through the
shared `redact/` module before sending; see [docs/security.md](security.md) for the privacy posture.

- **Crash reporter** (`crash_reporter/`): captures panics + signals, persists a report to disk, and offers to send it on
  the next launch. Targets `POST /crash-report` on `api.getcmdr.com`. For unexpected aborts only.
- **Error reporter** (`error_reporter/` + `error-reporter/`): captures everything else (MTP weirdness, network glitches,
  generic "this didn't work"). Two flows: user-initiated (**Help > Send error report…**, or the button on error toasts,
  see Flow A) and auto-send opt-in (`updates.errorReports`, see Flow B). Bundles the manifest + recent debug-level log
  tail (governed by `advanced.maxLogStorageMb`), redacts line-by-line, uploads to R2 via `POST /error-report`, returns a
  short `ERR-XXXXX` ID. Server posts a Discord notification with a 7-day presigned download link to a private
  `#error-reports` channel.

## Tooling and infrastructure

Dev workflow docs and external service references. All in `docs/tooling/`.

### Dev workflow

| Doc                                                  | Purpose                                                                        |
| ---------------------------------------------------- | ------------------------------------------------------------------------------ |
| [logging.md](tooling/logging.md)                     | Unified logging, `RUST_LOG` recipes for every subsystem                        |
| [css-health-checks.md](tooling/css-health-checks.md) | Stylelint + Go-based unused CSS checker                                        |
| [index-query.md](tooling/index-query.md)             | `index_query`: query index DB with `platform_case` collation (`sqlite3` can't) |

The check runner and E2E testing docs live colocated with their code:

- Check runner: [`scripts/check/CLAUDE.md`](../scripts/check/CLAUDE.md)
- E2E overview (all suites, fixtures): [`apps/desktop/test/CLAUDE.md`](../apps/desktop/test/CLAUDE.md)
- Playwright E2E (tauri-playwright, cross-platform):
  [`apps/desktop/test/e2e-playwright/CLAUDE.md`](../apps/desktop/test/e2e-playwright/CLAUDE.md)
- Linux E2E (Docker, VNC, legacy): [`apps/desktop/test/e2e-linux/CLAUDE.md`](../apps/desktop/test/e2e-linux/CLAUDE.md)

### Dependency management

[Renovate](https://docs.renovatebot.com/) (`renovate.json` in repo root) auto-updates all dependencies (npm, Cargo, Go).
Weekly grouped PRs for non-major updates (auto-merge), monthly for major (manual review). Security vulnerability patches
get immediate auto-merging PRs regardless of schedule.

### External services

| Doc                                      | Purpose                                                    |
| ---------------------------------------- | ---------------------------------------------------------- |
| [hetzner-vps.md](tooling/hetzner-vps.md) | Production VPS: SSH access, layout, deploy commands        |
| [umami.md](tooling/umami.md)             | Website analytics: API access, DB queries, troubleshooting |
| [cloudflare.md](tooling/cloudflare.md)   | Cmdr zones, workers, Pages, D1 telemetry                   |
| [posthog.md](tooling/posthog.md)         | Cmdr project ID and settings                               |
| [monitoring.md](tooling/monitoring.md)   | UptimeRobot: uptime checks, alerts                         |

ONLY do read-only operations with these services unless specifically asked to make changes.
