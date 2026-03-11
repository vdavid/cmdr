# Linux E2E tests (Docker + tauri-driver)

WebDriverIO E2E tests for Cmdr on Linux, using tauri-driver with WebKitGTK's WebKitWebDriver.

This is the workhorse test suite. All platform-independent app logic lives here: dialog flows, keyboard nav, selection,
view modes, file viewer, settings, command palette, and file operations. macOS tests only cover platform integration
(APFS ops, volume detection).

## Architecture

```
WebDriverIO --HTTP:4444--> tauri-driver --> WebKitWebDriver --> WebKitGTK (in-app)
```

Runs inside a Docker container (Ubuntu 24.04) with Xvfb for headless GUI. The host's source code is mounted in, so the
Tauri app is built inside the container.

## Running

```bash
cd apps/desktop

# Test-only changes (.ts test files, wdio.conf.ts):
# Source is mounted from host — just re-run, no rebuild needed.
pnpm test:e2e:linux

# Rust or Svelte code changes:
# Remove the target volume (keeps cargo registry cache, so recompilation is fast).
docker volume rm cmdr-target-cache && pnpm test:e2e:linux

# Nuclear option — nuke everything (cargo cache, target, node_modules):
./scripts/e2e-linux.sh --clean && pnpm test:e2e:linux

pnpm test:e2e:linux:build          # force rebuild Docker IMAGE (Dockerfile changes only)
pnpm test:e2e:linux:shell          # interactive shell in container for debugging
pnpm test:e2e:linux:native         # native Linux only (requires Tauri prereqs)
```

## Fixture system

Tests use a shared fixture helper (`../e2e-shared/fixtures.ts`) that creates a temp directory tree at
`/tmp/cmdr-e2e-<timestamp>/` with `left/` (text files, sub-dir, hidden file, bulk .dat files) and `right/` (empty).

The `CMDR_E2E_START_PATH` env var tells the app where to open. Fixtures are fully recreated before each test via
`recreateFixtures()` in the `beforeTest` hook so tests don't affect each other.

## Docker environment

The Docker container (`docker/Dockerfile`) includes: Ubuntu 24.04, WebKitGTK runtime + dev packages, X11 libs, Xvfb
(virtual framebuffer), dbus-x11 (required for WebKitGTK), Node.js + pnpm, Rust toolchain + tauri-driver.

### Build caching

| Volume                            | Contents                       | Remove to force...                        |
| --------------------------------- | ------------------------------ | ----------------------------------------- |
| `cmdr-cargo-cache`                | Cargo registry + compiled deps | Full crate re-download                    |
| `cmdr-target-cache`               | Compiled Tauri binary          | App recompilation (fast with cargo cache) |
| `cmdr-root-node-modules-cache`    | Root `node_modules/`           | `pnpm install`                            |
| `cmdr-desktop-node-modules-cache` | Desktop `node_modules/`        | `pnpm install`                            |

Most common operation: `docker volume rm cmdr-target-cache` after Rust/Svelte changes.

Why two node_modules volumes? Both must be Docker volumes to prevent Linux binaries from contaminating the host's
node_modules (which would break macOS smoke tests).

### Interactive debugging

```bash
pnpm test:e2e:linux:shell
# Inside the container: $TAURI_BINARY to run app, echo $DISPLAY to check display
```

### Watching tests live via VNC

1. Start the interactive shell: `pnpm test:e2e:linux:shell`
2. Inside the container: `x11vnc -display :99 -forever -nopw -rfbport 5900 -passwd "aaaa" &`
3. On your Mac, Finder > Cmd+K > `vnc://localhost:5900` (password: `aaaa`)
4. Run tests: `pnpm test:e2e:linux:native -- --spec test/e2e-linux/file-operations.spec.ts`

### VNC mode (visual debugging with hot reload)

```bash
pnpm test:e2e:linux:vnc
```

Opens a noVNC browser at http://localhost:6090/vnc.html?autoconnect=true. Runs `pnpm dev` inside Docker with Xvfb +
x11vnc + noVNC. Source is mounted from host, so `.svelte`/`.ts` edits trigger Vite HMR. Rust changes require restarting.
Useful for debugging Linux/WebKitGTK-specific behavior.

## WebKitGTK WebDriver quirks

These are critical for writing tests. Without these workarounds, tests will silently fail.

### 1. Native clicks fail on non-form elements

WebKitGTK's WebDriver rejects clicks on non-interactive container elements. **Use `jsClick()` (JS `el.click()`)
instead:**

```typescript
async function jsClick(element: WebdriverIO.Element): Promise<void> {
    await browser.execute((el: HTMLElement) => el.click(), element as unknown as HTMLElement)
}
```

### 2. `browser.keys(' ')` doesn't deliver Space

The Space character hits a CharKey/VirtualKey ambiguity in WebKitWebDriver. **Use the W3C Actions API instead:**

```typescript
await browser.action('key').down(' ').pause(50).up(' ').perform()
await browser.releaseActions()
```

### 3. Backspace must use JS `dispatchEvent` on the container

Neither `browser.keys('Backspace')` nor the W3C Actions API (`\uE003`) reliably deliver Backspace on WebKitGTK (native
runner or VM). **Dispatch on `.dual-pane-explorer`** (where `onkeydown` is bound):

```typescript
await browser.execute(() => {
    const container = document.querySelector('.dual-pane-explorer') as HTMLElement | null
    container?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true }))
})
```

**Caveat:** Synthetic `dispatchEvent` Backspace from a nested directory may navigate two levels up instead of one (lands
at the fixture root instead of the immediate parent). The Backspace test accepts either landing as valid.

### 4. Use `ctrlKey`, not `metaKey`, for Linux shortcuts

On Linux, `metaKey` maps to the Super/Windows key, not Ctrl. The shortcut system formats it as `Super+Shift+P` which
won't match `Ctrl+Shift+P`. Always use `ctrlKey: true` when dispatching keyboard events in Linux E2E tests.

## Ubuntu test VM (native Linux testing)

A local Ubuntu VM for testing Cmdr on Linux without Docker. Used for manual testing, debugging WebKitGTK-specific
behavior, and E2E test development with faster iteration.

### Quick start

```bash
# From macOS — SSH into the VM
ssh veszelovszki@192.168.1.97

# Inside the VM — run Cmdr
eval "$(mise activate bash)"
cd ~/cmdr
WEBKIT_DISABLE_COMPOSITING_MODE=1 pnpm dev
```

For GUI interaction (pressing keys, clicking), use the VM window in UTM directly — not SSH.

### VM specs

| Property | Value                             |
| -------- | --------------------------------- |
| Host app | UTM (Apple Virtualization engine) |
| OS       | Ubuntu 25.10 (aarch64)            |
| RAM      | 12 GB                             |
| CPU      | 6 cores                           |
| Disk     | 64 GB                             |
| Username | `veszelovszki`                    |
| SSH      | `ssh veszelovszki@192.168.1.97`   |
| IP       | `192.168.1.97` (static, on LAN)   |

### File layout

| Path                          | What                                        |
| ----------------------------- | ------------------------------------------- |
| `/mnt/cmdr/cmdr`              | VirtioFS mount of the macOS `cmdr` repo     |
| `~/cmdr`                      | Symlink to `/mnt/cmdr/cmdr`                 |
| `~/cmdr-node-modules/root`    | VM-local `node_modules` for monorepo root   |
| `~/cmdr-node-modules/desktop` | VM-local `node_modules` for `apps/desktop`  |
| `~/cmdr-target`               | VM-local `target/` for Rust build artifacts |

The macOS `cmdr` directory is shared via UTM's VirtioFS (`/etc/fstab`: `share /mnt/cmdr virtiofs defaults 0 0`). Edits
on either side are instant — Vite HMR picks up changes in ~1-3s.

Linux and macOS need different native binaries in `node_modules` and `target/`. The VM bind-mounts local directories
over the shared paths (configured in `/etc/fstab`). This prevents Linux binaries from contaminating the macOS filesystem
and avoids cross-platform cargo cache thrashing. Rebuild node_modules with:
`rm -rf ~/cmdr-node-modules/root/* ~/cmdr-node-modules/desktop/* && cd ~/cmdr && pnpm install`

### Toolchain

Managed by [mise](https://mise.jdx.dev/) — versions from `.mise.toml`. Always activate before running commands:
`eval "$(mise activate bash)"`. This is in `~/.bashrc` for interactive shells, but not SSH one-liners.

Rust via rustup (`rust-toolchain.toml`), Node/pnpm/Go via mise. System packages (Tauri prereqs, WebKitGTK dev libs) via
apt.

### Running commands via SSH (for agents)

`mise activate` doesn't work in non-interactive SSH one-liners. Use explicit PATH setup instead. Three env vars are
required:

- `DISPLAY=:0` — without it, `@wdio/xvfb` (bundled with `@wdio/local-runner`) auto-wraps worker processes with
  `xvfb-run`, which breaks Node.js IPC channels (`Error: write EINVAL` on `process.send()`).
- `XAUTHORITY` — SSH sessions don't inherit X11 auth. Point it to the Mutter Xwayland auth file (path changes on each
  login, so use a `find` command).
- `WEBKIT_DISABLE_COMPOSITING_MODE=1` — skips GPU compositing (see "Common tasks" below).

**Always verify bind mounts first, then run `pnpm install`** — the VM bind-mounts its own `node_modules` and `target`
over the shared macOS ones. If these mounts aren't active, `pnpm install` will write Linux binaries into the macOS
filesystem and corrupt the host's `node_modules`. The bind mounts are in `/etc/fstab` with systemd dependencies on the
VirtioFS mount, so they should activate on boot — but always verify.

```bash
# SSH one-liner setup (paste this prefix before any command)
MISE_PATH="/home/veszelovszki/.local/share/mise/installs"
export PATH="$MISE_PATH/node/25.6.0/bin:$MISE_PATH/pnpm/10.29.2:$MISE_PATH/go/1.25.7/bin:$PATH"
export DISPLAY=:0
export XAUTHORITY=$(find /run/user/1000/ -name ".mutter-Xwaylandauth.*" 2>/dev/null | head -1)
export WEBKIT_DISABLE_COMPOSITING_MODE=1

# CRITICAL: verify bind mounts before any pnpm install or cargo build
mountpoint -q ~/cmdr/node_modules || { echo "FATAL: bind mounts not active — run: sudo mount -a"; exit 1; }

# Run E2E tests via SSH (full one-liner)
ssh veszelovszki@192.168.1.97 'MISE_PATH="/home/veszelovszki/.local/share/mise/installs" && export PATH="$MISE_PATH/node/25.6.0/bin:$MISE_PATH/pnpm/10.29.2:$MISE_PATH/go/1.25.7/bin:$PATH" && export DISPLAY=:0 && export XAUTHORITY=$(find /run/user/1000/ -name ".mutter-Xwaylandauth.*" 2>/dev/null | head -1) && export WEBKIT_DISABLE_COMPOSITING_MODE=1 && mountpoint -q ~/cmdr/node_modules || { echo "FATAL: bind mounts not active — run: sudo mount -a"; exit 1; } && cd ~/cmdr && pnpm install && cd apps/desktop && pnpm test:e2e:linux:native'
```

If mise tool versions change (check `.mise.toml`), update the paths above. Find installed paths with `mise where node`,
`mise where pnpm`, etc.

### Common tasks

```bash
cd ~/cmdr && WEBKIT_DISABLE_COMPOSITING_MODE=1 pnpm dev                          # dev mode (with hot reload)
cd ~/cmdr/apps/desktop && WEBKIT_DISABLE_COMPOSITING_MODE=1 pnpm test:e2e:linux:native  # E2E tests natively
cd ~/cmdr && ./scripts/check.sh                              # all checks
RUST_LOG=debug pnpm dev                                      # debug logging
```

The `WEBKIT_DISABLE_COMPOSITING_MODE=1` env var skips GPU compositing in the VM (avoids ~50s startup stall from
software-emulated GPU). Real Linux machines with a GPU don't need this.

### VM troubleshooting

- **Shared folder not mounted**: `sudo mount -a`
- **node_modules bind mounts not active**: `mountpoint -q ~/cmdr/node_modules || sudo mount -a`. **Never run
  `pnpm install` or `cargo build` without verifying first** — Linux binaries will leak into the macOS shared filesystem
  and corrupt the host's `node_modules` / `target`. Recovery: activate mounts, then on VM:
  `rm -rf ~/cmdr-node-modules/root/* ~/cmdr-node-modules/desktop/* && cd ~/cmdr && pnpm install`, and on macOS:
  `rm -rf node_modules apps/desktop/node_modules && pnpm install`
- **VM IP changed**: Check inside VM: `ip addr show | grep 'inet ' | grep -v 127.0.0.1`
- **pnpm/node not found**: `eval "$(mise activate bash)"` (interactive) or use the explicit PATH from "Running commands
  via SSH" above (non-interactive)
- **"OS file watch limit reached"**: Cmdr's indexer + Warp Terminal together can exhaust the default inotify limit
  (65,536), especially on VirtioFS mounts with large `node_modules`/`target` trees. The VM has
  `fs.inotify.max_user_watches=524288` set in `/etc/sysctl.d/99-inotify.conf`. If it gets reset, re-apply with:
  `echo "fs.inotify.max_user_watches=524288" | sudo tee /etc/sysctl.d/99-inotify.conf && sudo sysctl -p /etc/sysctl.d/99-inotify.conf`

## Files

| File                      | Purpose                                                         |
| ------------------------- | --------------------------------------------------------------- |
| `wdio.conf.ts`            | WebDriverIO config: spawns tauri-driver, manages fixtures       |
| `app.spec.ts`             | 14 tests: rendering, keyboard nav, mouse interaction, dialogs   |
| `file-operations.spec.ts` | 8 tests: copy, move, rename, mkdir, view modes, hidden, palette |
| `file-watching.spec.ts`   | 1 test: inotify file watching (external mkdir detection)        |
| `settings.spec.ts`        | 5 tests: settings panel                                         |
| `viewer.spec.ts`          | 10 tests: file viewer                                           |
| `docker/Dockerfile`       | Ubuntu 24.04 image with Tauri prereqs, Xvfb, Node, Rust         |
| `docker/entrypoint.sh`    | Xvfb/dbus setup for headless GUI                                |
| `tsconfig.json`           | TypeScript config for WDIO types                                |

## CI integration

| Check nickname        | What it runs                                    | Included by default? |
| --------------------- | ----------------------------------------------- | -------------------- |
| `rust-tests-linux`    | `cargo test` in Docker (Linux/GTK)              | No (slow)            |
| `desktop-e2e-linux`   | Full E2E in Docker (WebDriverIO + tauri-driver) | No (slow)            |
| `e2e-linux-typecheck` | TypeScript check on E2E test files              | Yes                  |

Run slow checks explicitly: `./scripts/check.sh --check desktop-e2e-linux`

## Related

- Shared fixture helper: `test/e2e-shared/fixtures.ts`
- macOS E2E tests: `test/e2e-macos/` (platform integration only — APFS ops, volume detection)
- Linux stubs: `src-tauri/src/stubs/` (volumes, network, permissions use stubs on Linux)
