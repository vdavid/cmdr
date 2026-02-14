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
| `command-palette/` | Fuzzy command search (~45 commands) |
| `licensing/` | License validation, commercial reminders, expiration modals |
| `ai/` | Local LLM features (folder suggestions), download flow |
| `mtp/` | MTP (Android device) file browsing UI |
| `updates/` | Auto-updater UI |
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
| `mcp/` | MCP server (18 tools, YAML resources, agent-centric API) |
| `ai/` | llama-server lifecycle, model download, inference client |
| `licensing/` | Ed25519 license verification, server validation |
| `settings/` | Settings persistence (tauri-plugin-store) |
| `font_metrics/` | Binary font metrics cache, per-directory width calculation |
| `volumes/` | Volume abstraction (local, network, MTP) |
| `drag_image_detection.rs` | macOS method swizzle for drag image size detection |
| `drag_image_swap.rs` | Rich/transparent drag image swap for self-drags |
| `commands/` | Tauri command definitions (IPC entry points) |

## Other apps

| Directory | Purpose |
|-----------|---------|
| `apps/license-server/` | Cloudflare Worker + Hono. Paddle webhooks, Ed25519 key generation |
| `apps/website/` | getcmdr.com marketing site (Astro + Tailwind v4) |

## Cross-cutting patterns

### Data flow: frontend ↔ backend

File data lives in Rust (`LISTING_CACHE`). Frontend fetches visible ranges on-demand via IPC (`getFileRange`).
This avoids serializing 50k+ entries. Virtual scrolling renders only ~50 visible items.

### Persistence

- **App status** (`app-status.json`): ephemeral state — paths, focused pane, view modes, last-used paths per volume
- **Settings** (`settings-v2.json`): preferences — hidden files, density, date format. Registry-validated.
- **Shortcuts** (`shortcuts.json`): delta-only — only customizations stored, defaults in code
- **License** (`license.json`): activation state, timestamps
- **Window state**: `@tauri-apps/plugin-window-state` for size/position per window label

Philosophy: status is "where you are" (ephemeral), settings are "how you like it" (preferences).

### macOS specifics

- **Full Disk Access**: checked via `~/Library/Mail` readability (<5ms). Prompt on first launch.
- **Keychain**: stores network credentials and trial state. Uses `security-framework` crate.
- **copyfile(3)**: preserves xattrs, ACLs, resource forks. `COPYFILE_CLONE` for instant APFS clones.
- **ptpcamerad**: auto-claims USB devices. MTP shows workaround dialog with Terminal command.

### Dev mode

- `pnpm dev` at repo root for hot-reloading Tauri app
- AI disabled unless `CMDR_REAL_AI=1` (prevents large downloads)
- License mock via `CMDR_MOCK_LICENSE=commercial`
- MCP server available at `localhost:9224` for agent testing
- `withGlobalTauri: true` in dev mode — security risk if loading remote content

### Checker script

Go-based unified runner (`scripts/check/`). Parallel execution with dependency graph.
Coverage: 70% threshold enforced, `coverage-allowlist.json` exempts Tauri/DOM-dependent files.
