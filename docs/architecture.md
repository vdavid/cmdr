# Architecture

Map of Cmdr's major subsystems. Each directory has detailed docs in their `CLAUDE.md` file!

## Frontend (Svelte 5 + TypeScript)

All under `apps/desktop/src/lib/`.

| Directory | Purpose |
|-----------|---------|
| `file-explorer/` | Dual-pane file explorer — pane orchestration, selection, navigation, sorting |
| `file-explorer/views/` | Virtual-scrolling file lists (Brief + Full modes), 100k+ file support |
| `file-explorer/drag/` | Native drag-and-drop (drag-out, drop-in, pane-to-pane, macOS image swizzle) |
| `file-explorer/rename/` | Inline rename with validation, conflict resolution, extension change |
| `file-explorer/selection/` | Space/Shift/Cmd selection, range operations |
| `file-explorer/navigation/` | Back/forward history, breadcrumb, path utilities |
| `file-explorer/network/` | Network browser UI (SMB share browsing, login form) |
| `file-operations/` | Transfer dialogs (copy/move/mkdir) with progress and conflict resolution |
| `file-viewer/` | Read-only file viewer (opens in separate window, virtual scrolling) |
| `settings/` | Settings UI + registry-based architecture, reactive state |
| `shortcuts/` | Keyboard shortcut customization, scope hierarchy, conflict detection |
| `tauri-commands/` | Typed TypeScript wrappers for all Tauri IPC commands and events |
| `command-palette/` | Fuzzy command search (~45 commands) |
| `commands/` | Command registry (~50 commands), fuzzy search engine for command palette |
| `licensing/` | License validation, commercial reminders, expiration modals |
| `logging/` | Unified logging: LogTape config, batching bridge to Rust, verbose toggle |
| `ai/` | Local LLM features (folder suggestions), download flow |
| `indexing/` | Drive index state, events, priority triggers, scan status overlay |
| `mtp/` | MTP (Android device) file browsing UI |
| `onboarding/` | Full Disk Access prompt for first-launch onboarding |
| `ui/` | Shared UI primitives: ModalDialog, Button, AlertDialog, LoadingIcon, Notification, dialog registry |
| `updates/` | Auto-updater UI |
| `utils/` | Filename validation, confirm dialog utilities |
| `font-metrics/` | Character width measurement for accurate Brief mode column sizing |

## Backend (Rust + Tauri 2)

All under `apps/desktop/src-tauri/src/`.

| Directory/file | Purpose |
|----------------|---------|
| `file_system/listing/` | Directory reading, streaming, caching, sorting — serves virtual scroll |
| `file_system/write_operations/` | Copy/move/delete with safety patterns (temp+rename, staging, rollback) |
| `file_viewer/` | Three-backend file viewer (FullLoad, ByteSeek, LineIndex) |
| `network/` | SMB: mDNS discovery, share listing (smb-rs + smbutil), mounting, Keychain |
| `mtp/` | MTP device management, file ops, event-based watching |
| `mcp/` | MCP server (19 tools, YAML resources, agent-centric API) |
| `ai/` | llama-server lifecycle, model download, inference client |
| `licensing/` | Ed25519 license verification, server validation |
| `settings/` | Settings persistence (tauri-plugin-store) |
| `indexing/` | Background drive indexing (SQLite, jwalk, FSEvents), recursive directory sizes |
| `font_metrics/` | Binary font metrics cache, per-directory width calculation |
| `volumes/` | Volume abstraction (local, network, MTP), scanner/watcher traits |
| `stubs/` | Linux compilation stubs for macOS-only modules (used by Docker E2E pipeline) |
| `menu/` | Native menu bar: platform-specific construction, dispatch mapping, accelerator sync, context-aware enable/disable |
| `drag_image_detection.rs` | macOS method swizzle for drag image size detection |
| `drag_image_swap.rs` | Rich/transparent drag image swap for self-drags |
| `commands/` | Tauri command definitions (IPC entry points) |
| `capabilities/` | Per-window Tauri API permissions — must be updated when using new Tauri APIs from a window |
| `icons/` | App icons for all platforms + macOS Tahoe Liquid Glass (Assets.car). See [CLAUDE.md](../apps/desktop/src-tauri/icons/CLAUDE.md) for regeneration steps |

## Other apps

| Directory | Purpose |
|-----------|---------|
| `apps/license-server/` | Cloudflare Worker + Hono. Paddle webhooks, Ed25519 key generation. See [CLAUDE.md](../apps/license-server/CLAUDE.md) (technical reference) and [README](../apps/license-server/README.md) (first-time setup) |
| `apps/website/` | getcmdr.com marketing site (Astro + Tailwind v4). See [README](../apps/website/README.md) and [CLAUDE.md](../apps/website/CLAUDE.md) |
| `scripts/check/` | Go unified check runner (~40 checks, parallel with dependency graph) |

## Cross-cutting patterns

For detailed architecture patterns (data flow, navigation lifecycle, listing lifecycle, concurrency guards, cancellation,
volume mount/unmount, error recovery, persistence), see [architecture-patterns.md](architecture-patterns.md). Read the
relevant section when working on navigation, file operations, or volumes.

### Platform constraints

Rules that cut across many modules. All existing commands follow these — apply them to new code too.

1. **Tauri IPC threading.** Synchronous `#[tauri::command]` functions block the IPC handler thread.
   If one command hangs (e.g., a filesystem syscall on a dead network mount), ALL subsequent IPC
   calls from the frontend queue behind it and the app appears frozen. All filesystem-touching
   commands are `async` with `blocking_with_timeout` (2s default). When adding new commands that
   touch the filesystem, follow this pattern — see `commands/file_system.rs` for examples.

2. **Network mount blocking syscalls.** `statfs`, `readdir`, `metadata()`, NSURL resource queries,
   and `realpath` can all block indefinitely on slow/hung network mounts (kernel waits 30–120s).
   Every Tauri command that calls these is wrapped in `blocking_with_timeout`. New commands MUST
   do the same. See `docs/specs/blocking-ipc-hardening-plan.md` for the full audit.

3. **Two-layer timeout defense.** Backend: `blocking_with_timeout` (2–15s) wraps syscalls in
   `tokio::time::timeout`. Frontend: `withTimeout` (500ms–3s) races IPC calls and returns a
   fallback on expiry. Both layers are applied for critical paths (volume switching, path
   resolution, volume space queries). Apply both when adding new IPC calls to slow paths.

### macOS specifics

- **Full Disk Access**: checked via `~/Library/Mail` readability (<5ms). Prompt on first launch.
- **Keychain**: stores network credentials and trial state. Uses `security-framework` crate.
- **copyfile(3)**: preserves xattrs, ACLs, resource forks. `COPYFILE_CLONE` for instant APFS clones.
- **ptpcamerad**: auto-claims USB devices. MTP shows workaround dialog with Terminal command.

### Dev mode

- `pnpm dev` at repo root for hot-reloading Tauri app
- License mock via `CMDR_MOCK_LICENSE=commercial`
- MCP server available at `localhost:9224` for agent testing
- `withGlobalTauri: true` in dev mode — security risk if loading remote content

### Checker script

Go-based unified runner (`scripts/check/`). Parallel execution with dependency graph.
Coverage: 70% threshold enforced, `coverage-allowlist.json` exempts Tauri/DOM-dependent files.

## Tooling and infrastructure

Dev workflow docs and external service references. All in `docs/tooling/`.

### Dev workflow

| Doc | Purpose |
|-----|---------|
| [logging.md](tooling/logging.md) | Unified logging, `RUST_LOG` recipes for every subsystem |
| [css-health-checks.md](tooling/css-health-checks.md) | Stylelint + Go-based unused CSS checker |
| [index-query.md](tooling/index-query.md) | `index_query` — query index DB with `platform_case` collation (`sqlite3` can't) |

The check runner and E2E testing docs live colocated with their code:
- Check runner: [`scripts/check/CLAUDE.md`](../scripts/check/CLAUDE.md)
- E2E overview (why three suites, fixtures): [`apps/desktop/test/CLAUDE.md`](../apps/desktop/test/CLAUDE.md)
- Linux E2E (Docker, VNC, Ubuntu VM): [`apps/desktop/test/e2e-linux/CLAUDE.md`](../apps/desktop/test/e2e-linux/CLAUDE.md)
- macOS E2E (CrabNebula): [`apps/desktop/test/e2e-macos/CLAUDE.md`](../apps/desktop/test/e2e-macos/CLAUDE.md)

### Dependency management

[Renovate](https://docs.renovatebot.com/) (`renovate.json` in repo root) auto-updates all dependencies (npm, Cargo,
Go). Weekly grouped PRs for non-major updates (auto-merge), monthly for major (manual review). Security vulnerability
patches get immediate auto-merging PRs regardless of schedule.

### External services

| Doc | Purpose |
|-----|---------|
| [hetzner-vps.md](tooling/hetzner-vps.md) | Production VPS: SSH access, layout, deploy commands |
| [umami.md](tooling/umami.md) | Website analytics: API access, DB queries, troubleshooting |
| [cloudflare.md](tooling/cloudflare.md) | DNS, Workers, API token, download tracking (Analytics Engine) |
| [posthog.md](tooling/posthog.md) | Session replay and heatmaps (EU instance), API access |
| [paddle.md](tooling/paddle.md) | Payments API (live + sandbox), common operations |
| [ngrok.md](tooling/ngrok.md) | Tunnels for webhook testing |
| [monitoring.md](tooling/monitoring.md) | UptimeRobot: uptime checks, alerts |

ONLY do read-only operations with these services unless specifically asked to make changes.
