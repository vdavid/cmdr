# MTP E2E tests via virtual device

## Goal

Add E2E tests for Cmdr's MTP feature using mtp-rs 0.5.1's virtual device, so we can test device discovery, browsing,
file operations, free space display, read-only storage, and file watching — all without a real phone.

## Design approach

Two design principles work together:

1. **Invisible virtual device**: Register the virtual device before the MTP watcher starts. Cmdr's existing discovery →
   auto-connect → Volume registration flow handles everything. No test-only Tauri commands, no event bypasses.

2. **MCP-driven tests where possible**: Use the MCP server for navigation and file operations (cleaner, more robust than
   DOM manipulation). Use DOM queries only for visual assertions. This also closes a real gap — MCP currently can't
   interact with MTP volumes at all.

### How it works

1. On startup (feature-gated), Rust code creates a backing directory with test files and calls
   `mtp_rs::register_virtual_device()`.
2. `start_mtp_watcher()` snapshots current devices — virtual device is included.
3. Frontend's `mtp-store.initialize()` calls `scanDevices()` → `listMtpDevices()` → finds virtual device →
   auto-connects.
4. `MtpConnectionManager::connect()` calls `open_by_location()` — mtp-rs finds the registered virtual device.
5. Write probe, Volume registration, event loop — all run normally against the virtual device.
6. Tests use MCP tools (`select_volume`, `nav_to_path`, `copy`, `move`, etc.) to navigate and operate on the MTP volume,
   and DOM queries to verify visual state.

**Why this is elegant**: The virtual device is invisible to Cmdr. The MCP additions are genuine improvements (agents
couldn't use MTP at all before), not test scaffolding. The tests exercise the full stack: MCP → Tauri events → frontend
→ Volume trait → MTP connection manager → virtual device.

### mtp-rs API (verified)

The mtp-rs 0.5.1 virtual device API has been implemented and verified. Key facts:

- `register_virtual_device(&config)` adds the device to a global registry
- `MtpDevice::list_devices()` includes registered virtual devices (discovery works)
- `MtpDevice::builder().open_by_location(id)` / `.open_by_serial(serial)` opens virtual devices
- All standard operations work (list, download, upload, delete, create_folder, rename, move, copy, next_event)
- Virtual device location IDs are in the `0xFFFF_0000_0000_0000+` range
- `read_only: true` on a storage config enforces read-only at the MTP protocol level

---

## Milestone 1: Virtual device backend wiring (~100 lines of Rust)

### 1a. Add `virtual-mtp` feature flag

In `apps/desktop/src-tauri/Cargo.toml`, add to `[features]`:

```toml
virtual-mtp = ["mtp-rs/virtual-device"]
```

**Why a separate flag** (not bundled into `playwright-e2e`): The MTP tests need the virtual device, but existing
Playwright tests don't. Keeping them separate means existing tests still build without pulling in the virtual device
code. The E2E build command becomes `--features playwright-e2e,virtual-mtp`.

### 1b. Create virtual device setup module

New file: `apps/desktop/src-tauri/src/mtp/virtual_device.rs`

Gated behind `#[cfg(feature = "virtual-mtp")]`. On call:

1. Create a backing directory at `/tmp/cmdr-mtp-e2e-fixtures/` with two subdirectories (one per storage):
   - `internal/` — writable storage, pre-populated with test files
   - `readonly/` — read-only storage, pre-populated with a few files
2. Pre-populate `internal/` with:
   - `Documents/report.txt` (small text file)
   - `Documents/notes.txt` (small text file)
   - `DCIM/photo-001.jpg` (small dummy file — content doesn't matter, just needs to exist)
   - `Music/` (empty directory)
3. Pre-populate `readonly/` with:
   - `photos/sunset.jpg` (small dummy file)
4. Call `mtp_rs::register_virtual_device(VirtualDeviceConfig { ... })` with:
   - manufacturer: `"Google"`, model: `"Virtual Pixel 9"`, serial: `"cmdr-e2e-virtual"`
   - Two storages: "Internal Storage" (writable, 64 GB capacity) and "SD Card" (read-only, 16 GB capacity)
   - `event_poll_interval: Duration::from_millis(100)` (fast enough for tests, not instant)
5. Return the registered device info (location_id) for potential cleanup.
6. Export the backing dir path as a constant: `pub const MTP_FIXTURE_ROOT: &str = "/tmp/cmdr-mtp-e2e-fixtures"` — the
   TypeScript fixture helper references this same path. A comment in both files points to the other.

**Why two storages**: Tests the multi-storage UI (each storage as a separate volume in the picker) and the read-only
behavior path. This is a realistic setup — many Android devices report two storages.

**Why pre-populated files**: The tests need something to browse, copy, move, and delete. Creating the files at startup
(not in TypeScript fixtures) is necessary because mtp-rs's virtual device serves them via the MTP protocol from the
backing dir.

### 1c. Hook into app startup

In `apps/desktop/src-tauri/src/lib.rs`, right before `start_mtp_watcher()`:

```rust
#[cfg(feature = "virtual-mtp")]
mtp::virtual_device::setup_virtual_mtp_device();
```

**Why before the watcher**: The watcher's first action is to snapshot `KNOWN_DEVICES`. The virtual device must be
registered before that, so it appears in the initial snapshot. Then `scanDevices()` on the frontend finds it.

### 1d. Wire up module

In `apps/desktop/src-tauri/src/mtp/mod.rs`, add:

```rust
#[cfg(feature = "virtual-mtp")]
pub mod virtual_device;
```

### After this milestone

Run `./scripts/check.sh --check clippy --check rustfmt` to verify compilation.

Run a manual test: build with `--features playwright-e2e,virtual-mtp`, start the app, and verify:

1. Virtual device appears in the volume picker under "Mobile".
2. Both storages show as separate entries (for example "Virtual Pixel 9 - Internal Storage" and "Virtual Pixel 9 - SD
   Card"). **Note the exact text** — the MCP `select_volume` will need it.
3. Browsing the writable storage shows the pre-populated files.
4. The read-only storage (SD Card) is actually detected as read-only — check the log for `access_capability=ReadOnly`
   and `is_read_only=true` on the SD Card storage (lines 608-611 of `connection/mod.rs`). Note: if mtp-rs reports the
   `AccessCapability` as read-only, the write probe is skipped entirely (the `storage_reports_read_only` branch at line
   588-591). So the expected log is the storage info line, not the probe line.
5. **Check the device ID** in the volume picker DOM or logs — virtual devices get location IDs in the
   `0xFFFF_0000_0000_0000+` range. The decimal representation exceeds JavaScript's `Number.MAX_SAFE_INTEGER`. Verify the
   ID is handled as a string everywhere and never parsed as a number.
6. Note the exact volume names and DOM structure for Milestone 2's MCP work.

---

## Milestone 2: MCP support for MTP volumes

Currently, the MCP server can't interact with MTP volumes at all — `select_volume` rejects them, `nav_to_path` fails on
`mtp://` paths, and `cmdr://state` doesn't list them. This milestone fixes that. These are genuine improvements, not
test scaffolding — agents need MTP access.

### Current state (what's broken)

| Tool/Resource                  | What happens with MTP              | Why                                                                   |
| ------------------------------ | ---------------------------------- | --------------------------------------------------------------------- |
| `select_volume`                | Rejects with "Volume not found"    | Validates against `list_locations()` which doesn't include MTP        |
| `nav_to_path`                  | Rejects with "Path does not exist" | Uses `Path::new(path).exists()` which fails for `mtp://` paths        |
| `cmdr://state` volumes section | MTP volumes missing                | Only shows `list_locations()` results                                 |
| `copy`/`move`/`delete`/`mkdir` | Should work                        | Fire-and-forget to frontend; frontend dialogs operate on current pane |
| `await` tool                   | Should work                        | Polls `PaneStateStore` which already syncs `volume_id` from frontend  |

### 2a. Add MTP volumes to `cmdr://state`

In `resources.rs` (~line 326-340), after the `list_locations()` loop, add MTP volumes from the connected device
registry. Each connected MTP storage should appear as a volume entry with its name and volume ID.

**Why include the volume ID**: Tests need to pass the volume ID to `select_volume`. The `cmdr://state` resource is how
agents discover available volumes.

Also expose `volume_id` (not just `volume_name`) in the per-pane state section, so agents can see which volume a pane is
currently on.

### 2b. Extend `select_volume` for MTP

**Backend** (`executor.rs` ~line 395-457):

- Accept MTP volume names in addition to local volumes. The `#[cfg(target_os = "macos")]` validation block at lines
  410-423 checks against `list_locations()`. Add a branch: if the name matches a connected MTP device's storage name
  (from `mtp::connection_manager().get_device_info()` or similar), accept it. On Linux, the validation block doesn't
  exist, so MTP names pass through automatically.

**Frontend** (`DualPaneExplorer.selectVolumeByName()` ~line 2026) — **must be extended** (not optional). Currently it
only searches the `volumes` array (local volumes). Without this change, `select_volume` will pass Rust validation, emit
the `mcp-volume-select` event, the frontend will call `selectVolumeByName`, fail to find the MTP volume, return `false`,
the path won't change, and the Rust side will time out after 30s. The fix: also search `getMtpVolumes()` for a matching
name, and if found, call `handleVolumeChange` with the MTP volume's ID and path.

### 2c. Extend `nav_to_path` for MTP paths

**Backend** (`executor.rs` ~line 458-489):

- Recognize `mtp://` paths and skip the `Path::new(path).exists()` check.
- Pass the path through to the frontend via the existing `mcp-nav-to-path` event.

**Frontend** (`DualPaneExplorer.navigateToPath()` ~line 1960-1968):

- **Remove the MTP rejection** (`if (volumeId.startsWith('mtp-'))` returns an error string). This check was added when
  MCP didn't support MTP, but now it should work — the pane can navigate to MTP paths via the Volume trait.
- **Require `select_volume` first**: If the pane is not currently on the matching MTP volume, return a clear error like
  "Pane is not on this MTP volume — call select_volume first". This is simpler than auto-switching and matches how the
  tests are structured (they always call `select_volume` then `nav_to_path`). The `mtp://` path contains the
  device/storage ID, so the check is: parse the volume ID from the path, compare with the pane's current `volumeId`.

### 2d. Verify `copy`/`move`/`delete`/`mkdir` work on MTP panes

These are fire-and-forget — they emit events and the frontend handles them. The frontend dialogs operate on whatever is
in the current pane. If the pane is showing MTP files, operations go through the Volume trait. **Just verify this works
during manual testing** — no code changes expected.

### After this milestone

Run `./scripts/check.sh --check clippy --check rustfmt`.

Manual test with the MCP via curl or `mcp-call.sh`:

```bash
# Read state — verify MTP volumes appear
./scripts/mcp-call.sh --read-resource 'cmdr://state'

# Select MTP volume
./scripts/mcp-call.sh select_volume '{"pane":"left","name":"Virtual Pixel 9 - Internal Storage"}'

# Navigate into a directory
./scripts/mcp-call.sh nav_to_path '{"pane":"left","path":"mtp://<device_id>:<storage_id>/Documents"}'

# Copy with auto-confirm
./scripts/mcp-call.sh copy '{"autoConfirm":true}'
```

---

## Milestone 3: MTP fixture management in TypeScript

### 3a. Create MTP fixture helper

New file: `apps/desktop/test/e2e-shared/mtp-fixtures.ts`

Functions:

- `recreateMtpFixtures()` — recreates the file structure in `/tmp/cmdr-mtp-e2e-fixtures/internal/` and `readonly/`.
  Deletes ALL contents of both directories (not just known files — tests like F7 mkdir create artifacts), preserves the
  root directories themselves (to keep the virtual device's backing dir inodes stable), then recreates the fixture file
  tree.
- `MTP_FIXTURE_ROOT` constant — `/tmp/cmdr-mtp-e2e-fixtures`. A comment points to the Rust constant at
  `src-tauri/src/mtp/virtual_device.rs::MTP_FIXTURE_ROOT`.
- Self-test block at the bottom (like `fixtures.ts` has) for standalone verification.

### 3b. Create MCP client helper

New file: `apps/desktop/test/e2e-shared/mcp-client.ts`

A lightweight wrapper around `fetch()` for calling the MCP server from E2E tests:

```ts
let mcpPort: number | null = null

/** Discovers the actual MCP port from the running app via Tauri IPC. */
export async function initMcpClient(tauriPage: PageLike): Promise<void> {
  mcpPort = await tauriPage.evaluate<number>(`window.__TAURI_INTERNALS__.invoke('get_mcp_port')`)
  if (!mcpPort) throw new Error('MCP server not running')
}

export async function mcpCall(tool: string, args: Record<string, unknown>): Promise<string> {
  if (!mcpPort) throw new Error('Call initMcpClient() first')
  const res = await fetch(`http://localhost:${mcpPort}/mcp`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'tools/call',
      params: { name: tool, arguments: args },
    }),
  })
  const json = await res.json()
  if (json.error) throw new Error(json.error.message)
  return json.result.content[0].text
}

export async function mcpReadResource(uri: string): Promise<string> {
  // similar, using resources/read
}
```

The port is discovered at test initialization via the `get_mcp_port` Tauri command, which returns the actual bound port
(handles the auto-probe case). Call `initMcpClient(tauriPage)` once in the test file's setup.

### 3c. Update global setup

Modify `apps/desktop/test/e2e-playwright/global-setup.ts` to also call `recreateMtpFixtures()` — ensures clean MTP state
at the start of the test run.

### After this milestone

Verify fixture helpers: `npx tsx apps/desktop/test/e2e-shared/mtp-fixtures.ts` Verify MCP client: start the app, run a
quick script that calls `mcpReadResource('cmdr://state')`.

---

## Milestone 4: E2E test spec

### 4a. Create MTP test file

New file: `apps/desktop/test/e2e-playwright/mtp.spec.ts`

#### Per-test fixture cleanup

```ts
test.beforeEach(async () => {
  recreateFixtures(getFixtureRoot()) // Local fixtures needed for cross-storage tests (MTP↔local)
  recreateMtpFixtures() // MTP backing dir: delete ALL contents, recreate fixture tree
  await sleep(500) // Let the virtual device's event loop settle
})
```

#### Test approach: MCP for operations, DOM for visual assertions

Most tests follow this pattern:

1. `ensureAppReady(tauriPage)` — route, loading screen, focus (navigates both panes to local fixtures).
2. MCP `select_volume` + `nav_to_path` — navigate pane(s) to MTP volumes/directories.
3. MCP `copy`/`move`/`delete`/`mkdir` with `autoConfirm` — perform file operations.
4. MCP `await` — wait for expected state (file appears, path changes).
5. DOM queries — verify visual elements (volume picker rendering, breadcrumb text, error dialogs).

A few tests use keyboard + DOM instead of MCP, to verify the full keyboard→MTP flow works.

#### Helper: discover MTP volume ID

```ts
async function getMtpVolumeId(storageName: string): Promise<string> {
  const state = await mcpReadResource('cmdr://state')
  // Parse YAML, find volume by name containing storageName, return its ID
}
```

Needed because the device ID is assigned at runtime. Tests call this once and reuse the ID.

#### Test: device appears in volume picker (DOM)

1. `ensureAppReady(tauriPage)`.
2. Open volume picker (click breadcrumb in left pane).
3. Assert a "Mobile" group exists in the DOM.
4. Assert both storages are listed ("Virtual Pixel 9 - Internal Storage", "Virtual Pixel 9 - SD Card" or similar).

**Why DOM, not MCP**: This tests the visual rendering of MTP devices in the volume picker — a UI concern.

#### Test: browse MTP device files and navigate back (MCP + DOM)

1. `ensureAppReady(tauriPage)`.
2. MCP: `select_volume({ pane: 'left', name: '<Internal Storage name>' })`.
3. MCP: `await({ pane: 'left', has_item: 'Documents' })`.
4. DOM: Assert `Documents`, `DCIM`, `Music` directories appear in left pane.
5. MCP: `nav_to_path({ pane: 'left', path: 'mtp://<id>:<storage>/Documents' })`.
6. MCP: `await({ pane: 'left', has_item: 'report.txt' })`.
7. DOM: Assert `report.txt` and `notes.txt` appear.
8. MCP: `nav_to_parent({ pane: 'left' })`.
9. MCP: `await({ pane: 'left', has_item: 'Documents' })`.

**Why**: Tests directory listing, path resolution, `PathHandleCache`, and parent navigation through the full stack.

#### Test: free space is displayed (DOM)

1. Navigate to MTP Internal Storage via MCP.
2. DOM: Check that the volume breadcrumb shows capacity info (non-zero, formatted).

**Why**: Tests `get_live_storage_space()` and the breadcrumb's space display for MTP volumes.

#### Test: copy file from MTP to local (MCP)

1. MCP: Navigate left pane to MTP Internal Storage → `Documents/`.
2. `ensureAppReady` already put right pane on local fixture `right/`.
3. MCP: `move_cursor({ pane: '...', filename: 'report.txt' })`.
4. MCP: `copy({ autoConfirm: true })`.
5. MCP: `await({ pane: 'right', has_item: 'report.txt' })`.
6. Node.js: Assert `report.txt` exists on disk at `{fixtureRoot}/right/report.txt`.
7. MCP: `switch_pane()`, `await({ pane: 'left', has_item: 'report.txt' })` — still exists (copy, not move).

**Why**: Tests MTP download through the Volume trait.

#### Test: copy file from local to MTP (MCP)

1. `ensureAppReady` puts left pane on local `left/` (has `file-a.txt`).
2. MCP: Navigate right pane to MTP Internal Storage root.
3. MCP: `move_cursor({ pane: '...', filename: 'file-a.txt' })`.
4. MCP: `copy({ autoConfirm: true })`.
5. MCP: `await` for `file-a.txt` in right pane.
6. Node.js: Assert `file-a.txt` exists in MTP backing dir (`/tmp/cmdr-mtp-e2e-fixtures/internal/file-a.txt`).

**Why**: Tests MTP upload path.

#### Test: move file on MTP (MCP)

1. MCP: Navigate left pane to MTP Internal Storage → `Documents/`.
2. MCP: Navigate right pane to MTP Internal Storage → `Music/`.
3. MCP: `switch_pane()` to ensure left pane is focused.
4. MCP: `move_cursor({ pane: '...', filename: 'notes.txt' })`.
5. MCP: `move({ autoConfirm: true })`.
6. MCP: `await({ pane: 'left', ... })` — wait for `notes.txt` to disappear from `Documents/`.
7. MCP: `await({ pane: 'right', has_item: 'notes.txt' })`.

**Why**: Tests MTP move_object. Both directories must be listed first to populate `PathHandleCache`.

#### Test: delete file on MTP (MCP)

1. MCP: Navigate to MTP Internal Storage → `Documents/`.
2. MCP: `move_cursor({ pane: '...', filename: 'report.txt' })`.
3. MCP: `delete({ autoConfirm: true })`.
4. MCP: `await` — wait for `report.txt` to disappear.
5. Node.js: Assert file gone from backing dir.

**Why**: Tests MTP delete (permanent, no Trash).

#### Test: create folder on MTP (MCP)

1. MCP: Navigate to MTP Internal Storage root.
2. MCP: `mkdir({ name: 'NewFolder' })` (if supported) or keyboard F7 + type + confirm.
3. MCP: `await({ pane: ..., has_item: 'NewFolder' })`.
4. Node.js: Assert directory exists in backing dir.

**Why**: Tests create_folder through the Volume trait.

#### Test: rename on MTP (keyboard + DOM)

1. MCP: Navigate to MTP Internal Storage → `Documents/`.
2. DOM: `moveCursorToFile(tauriPage, 'report.txt')`.
3. Keyboard: F2, clear input, type `renamed-report.txt`, Enter.
4. DOM: `pollUntil` for `renamed-report.txt` in pane, `report.txt` gone.

**Why**: No MCP rename tool exists, so this test uses keyboard. Also validates the keyboard→MTP flow.

#### Test: read-only storage rejects writes (keyboard + DOM)

1. MCP: Navigate to MTP SD Card (read-only storage), then into `photos/`.
2. DOM: Verify `sunset.jpg` is visible.
3. Keyboard: F7 (create folder) — expect an error dialog or disabled action.
4. DOM: Assert error state.
5. Keyboard: Cursor on `sunset.jpg`, attempt delete — expect an error dialog or disabled action.

**Why**: Tests that `is_read_only` propagates from the write probe through to UI behavior. Uses keyboard because we want
to test the user-facing error path.

**Note**: The exact UI behavior (error dialog, disabled buttons, etc.) depends on how Cmdr handles read-only volumes —
verify during Milestone 1 manual testing and make assertions specific.

#### Test: file watching / external change detection (MCP + Node.js)

1. MCP: Navigate to MTP Internal Storage → `Documents/`.
2. Node.js: `fs.writeFileSync('/tmp/cmdr-mtp-e2e-fixtures/internal/Documents/new-file.txt', 'hello')`.
3. MCP: `await({ pane: ..., has_item: 'new-file.txt' })` with generous 10s timeout.

**Why**: Tests the virtual device's event loop — it watches the backing dir and emits `ObjectAdded` events, which Cmdr's
event loop picks up and sends as `directory-diff` to the frontend.

### After this milestone

Run the full MTP test suite:

```bash
cd apps/desktop
pnpm test:e2e:playwright:build  # Rebuild with --features playwright-e2e,virtual-mtp
# Start app in one terminal, run tests in another
pnpm test:e2e:playwright
```

---

## Milestone 5: Linux Docker support

### 5a. Update Docker build command

In `apps/desktop/test/e2e-linux/e2e-linux.sh` (or wherever the Docker build command lives), change the features flag
from `--features playwright-e2e` to `--features playwright-e2e,virtual-mtp`.

### 5b. Verify Linux E2E

```bash
pnpm test:e2e:linux
```

MTP works on Linux, and the virtual device is pure Rust (no USB needed), so this should work without Docker
configuration changes.

### After this milestone

All MTP E2E tests pass on both macOS (native) and Linux (Docker).

---

## Milestone 6: Documentation

### 6a. Update CLAUDE.md files

- `apps/desktop/src-tauri/src/mtp/CLAUDE.md` — add virtual device module to file map, note the `virtual-mtp` feature
  flag
- `apps/desktop/src-tauri/src/mcp/CLAUDE.md` — document MTP volume support in `select_volume`, `nav_to_path`, and
  `cmdr://state`
- `apps/desktop/test/CLAUDE.md` — mention MTP E2E tests and the virtual device approach
- `apps/desktop/test/e2e-playwright/CLAUDE.md` — add `mtp.spec.ts` to the file table, note MCP client helper
- `apps/desktop/test/e2e-linux/CLAUDE.md` — update feature flags in architecture diagram

### 6b. Update build instructions

If the Playwright build instructions reference `--features playwright-e2e`, note that MTP tests additionally need
`virtual-mtp`.

---

## What we're NOT testing (and why)

- **USB hotplug detection**: Virtual device is registered before the watcher starts. Hotplug is nusb-level; testing it
  requires USB gadget support (kernel-level, not available in Docker). Hotplug is a thin layer (`watcher.rs`) — the real
  complexity is in discovery → connect → Volume → UI, which we DO test.
- **ptpcamerad / udev permission dialogs**: Platform-specific error paths triggered by real USB errors. Covered by
  frontend unit tests (`mtp-store.test.ts`).
- **Transfer progress UI**: Virtual device operations are near-instant (local disk). Progress UI is tested via unit
  tests.
- **Multi-device scenarios**: One virtual device validates the full pipeline. Multi-device adds complexity without
  testing different code paths.
- **File viewer (F3) on MTP files**: Can be added as a follow-up — tests viewer integration, not the core MTP pipeline.
- **Cross-storage copy**: SD Card is read-only, so Internal→SD is impossible. SD→Internal is a download, same code path
  as MTP-to-local.
- **Eject/disconnect**: Cmdr doesn't have an eject feature currently — out of scope.

## Risks and mitigations

- **JavaScript number precision for device IDs**: Virtual device location IDs (`0xFFFF_0000_0000_0000+` range) exceed
  `Number.MAX_SAFE_INTEGER`. The string `id` field (`"mtp-{location_id}"`) is safe, but `MtpDeviceInfo.location_id` is
  `u64` in Rust and `number` in TypeScript — serde serializes it as a JSON number that JavaScript will silently
  truncate. **Fix required**: either change `location_id` to serialize as a string (add
  `#[serde(serialize_with = "...")]` on the Rust side and update the TS type to `string`), or accept the truncation
  since `location_id` is only used for display/logging, never for device lookup (the string `id` is used for that).
  **Verify during Milestone 1**.
- **Read-only enforcement**: The plan assumes mtp-rs's `read_only: true` config enforces read-only at the MTP protocol
  level. If it only affects `AccessCapability` metadata, the write probe would mark it writable. **Verify during
  Milestone 1**. Fallback: make the backing dir OS-level read-only (`chmod`).
- **Event loop timing during fixture recreation**: When `recreateMtpFixtures()` deletes and recreates files, the virtual
  device's event loop fires `ObjectRemoved`/`ObjectAdded` events. **Mitigated** by 500ms settling wait in
  `test.beforeEach()`.
- **MCP port discovery**: The MCP client queries the actual port via the `get_mcp_port` Tauri command, so auto-probe is
  handled correctly.
- **`select_volume` timeout gotcha**: `select_volume` times out when re-selecting the same volume (polls for path
  change, sees none). Mitigated because `ensureAppReady()` navigates both panes to local fixtures before each test, so
  the subsequent `select_volume` to MTP always sees a path change.
- **Existing tests unaffected**: No existing E2E tests assert on the volume picker, so the virtual device appearing in
  the "Mobile" group during all tests won't break anything. Verified by grep.
