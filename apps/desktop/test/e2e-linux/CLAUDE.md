# Linux E2E Docker infrastructure

Docker setup for running the Playwright E2E tests (`../e2e-playwright/`) on Linux. The test files themselves live in
`e2e-playwright/` and are shared between macOS and Linux -- see `e2e-playwright/CLAUDE.md` for test documentation.

## Architecture

```
e2e-linux.sh
├─ Build Tauri binary in Docker (--features playwright-e2e,virtual-mtp,smb-e2e)
├─ Start SMB Docker containers (smb-consumer-guest, smb-consumer-auth)
├─ Launch E2E container on smb-consumer_default network
│   ├─ entrypoint.sh: Xvfb + dbus + GVFS + optional VNC
│   ├─ Create fixtures, start Tauri app (with SMB_E2E_*_HOST/PORT env vars)
│   ├─ Wait for /tmp/tauri-playwright.sock
│   └─ Run: npx playwright test --config test/e2e-playwright/playwright.config.ts
└─ Report results
```

## Running

```bash
cd apps/desktop

pnpm test:e2e:linux                    # Full run: build (if needed) + test in Docker
pnpm test:e2e:linux:build              # Force rebuild Docker image (Dockerfile changes only)
pnpm test:e2e:linux:shell              # Interactive shell in container
pnpm test:e2e:linux:vnc                # VNC mode with hot reload (pnpm dev)
./scripts/e2e-linux.sh --grep "SMB"    # Run only tests matching a pattern
```

## Build caching

| Volume                            | Contents                       | Remove to force...                        |
| --------------------------------- | ------------------------------ | ----------------------------------------- |
| `cmdr-cargo-cache`                | Cargo registry + compiled deps | Full crate re-download                    |
| `cmdr-target-cache`               | Compiled Tauri binary          | App recompilation (fast with cargo cache) |
| `cmdr-root-node-modules-cache`    | Root `node_modules/`           | `pnpm install`                            |
| `cmdr-desktop-node-modules-cache` | Desktop `node_modules/`        | `pnpm install`                            |

Most common operation: `docker volume rm cmdr-target-cache` after Rust/Svelte changes or feature flag changes.

All four volume names are overridable via `CARGO_VOLUME`, `TARGET_VOLUME`, `ROOT_NODE_MODULES_VOLUME`, and
`DESKTOP_NODE_MODULES_VOLUME` env vars. CI sets them to host bind-mount paths (`/tmp/cmdr-docker-cache/...`) so
`actions/cache` can persist them across runs (it can't cache Docker named volumes).

## Files

| File                   | Purpose                                                 |
| ---------------------- | ------------------------------------------------------- |
| `docker/Dockerfile`    | Ubuntu 24.04 image with Tauri prereqs, Xvfb, Rust, Node |
| `docker/entrypoint.sh` | Xvfb/dbus/GVFS/VNC setup for headless GUI               |

## SMB E2E networking

The E2E container joins the `smb-consumer_default` Docker network so it can reach the SMB containers
(`smb-consumer-guest:445`, `smb-consumer-auth:445`) by name. Containers come from smb2's consumer test harness. The
`e2e-linux.sh` script starts the SMB containers automatically and passes env vars (`SMB_E2E_GUEST_HOST`,
`SMB_E2E_GUEST_PORT`, etc.) to the Tauri app. The Rust `virtual_smb_hosts.rs` reads these to inject the correct
addresses. On macOS (local dev), smb2's default ports (10480/10481) are used instead.

The Docker image includes `smbclient` (for the `smb_smbclient.rs` fallback), `cifs-utils`, and GVFS packages (`gvfs`,
`gvfs-backends`, `gvfs-daemons`, `gvfs-fuse`). The entrypoint starts `gvfsd` so that `gio mount` works for user-space
SMB mounting -- this is what Cmdr's `mount_linux.rs` uses. Pre-mounting via `gio mount` produces the same GVFS paths
that the app expects (`/run/user/<uid>/gvfs/smb-share:server=<host>,share=<share>`).

The E2E container runs with `--privileged` because Docker's default seccomp profile blocks the `mount` syscall even with
`CAP_SYS_ADMIN`, and GVFS-FUSE needs `/dev/fuse`.

## Gotchas

**Gotcha**: Root volume is named "Root" on Linux, "Macintosh HD" on macOS. **Why**: Tests that emit `mcp-volume-select`
events to switch back to a local volume must use the correct name. `smb.spec.ts` and `mtp.spec.ts` have
`LOCAL_VOLUME_NAME` constants for this.

**Gotcha**: The `mcp-volume-select` event listener only exists on the file explorer route (`/`), not on `/settings`.
**Why**: If the previous test left the app on `/settings`, the `beforeEach` must navigate to `/` before emitting volume
select events. Otherwise the event is silently ignored.

**Gotcha**: GVFS requires D-Bus session bus and `gvfsd` running before any `gio mount` call. **Why**: The entrypoint
starts `dbus-launch` and `/usr/libexec/gvfsd` in that order. If gvfsd isn't running, `gio mount` silently fails or
hangs. `XDG_RUNTIME_DIR` must be `/run/user/<uid>` (not `/tmp/...`) for GVFS mount paths to match what
`mount_linux.rs`'s `derive_gvfs_path` computes.

**Gotcha**: Running all tests in sequence can cause the Tauri app to exit before SMB tests. **Why**: The accessibility
test opens the settings page and navigates through all sections including MCP settings. Combined with cross-window
setting sync, this can trigger an MCP server state change. When SMB tests start, the app may have already exited.
Running SMB tests in isolation avoids this. Investigating the root cause is tracked separately.

## CI integration

| Check nickname        | What it runs                             | Included by default? |
| --------------------- | ---------------------------------------- | -------------------- |
| `desktop-e2e-linux`   | Playwright E2E in Docker                 | No (slow)            |
| `e2e-linux-typecheck` | TypeScript check on e2e-playwright files | Yes                  |
