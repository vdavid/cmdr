# Add VNC + hot reload to Docker e2e-linux environment

## Context

We're debugging E2E test failures where F5/F7 function keys don't trigger dialogs in the
WebKitGTK-based Tauri app running in Docker. Blind iteration has been slow (~5 min per cycle,
6 synthetic event methods all fail). We need an interactive Linux GUI where we can see the app,
press keys, and observe behavior — while also seeing logs. Adding hot reload lets us iterate
on both the app code and test code without rebuilding.

## Approach

Add x11vnc + noVNC to the existing Docker container, and a --vnc mode that runs
pnpm dev (Vite + Tauri hot reload) inside the container. The user opens a browser tab to
see and interact with the full Cmdr GUI. Terminal streams logs. Code edits on macOS trigger
instant frontend hot reload via Vite HMR.

Ports use non-standard values to avoid clashing with other local services:

- 6090 — noVNC web client (browser-based)
- 5990 — native VNC (macOS Screen Sharing)

## Files to modify

1. apps/desktop/test/e2e-linux/docker/Dockerfile — Add x11vnc, novnc, websockify
2. apps/desktop/test/e2e-linux/docker/entrypoint.sh — Start VNC when VNC=1 is set
3. apps/desktop/scripts/e2e-linux.sh — Add --vnc flag: ports, env, runs pnpm dev
4. apps/desktop/package.json — Add test:e2e:linux:vnc script
5. docs/tooling/e2e-testing-guide.md — Document the VNC mode

## Detailed changes

1. Dockerfile — add VNC packages (~5 MB)
   Add to the existing apt-get install block:
    ```
    x11vnc \
    novnc \
    python3-websockify \
    ```
2. Entrypoint — conditionally start VNC
   After starting Xvfb and D-Bus, add:
    ```
    if [ "${VNC:-}" = "1" ]; then
    x11vnc -display :99 -forever -nopw -shared -rfbport 5990 -q &
    /usr/share/novnc/utils/novnc_proxy --vnc localhost:5990 --listen 6090 &>/dev/null &
    echo ""
    echo "╔══════════════════════════════════════════════════════════╗"
    echo "║ Open in browser: http://localhost:6090/vnc.html?autoconnect=true  ║"
    echo "╚══════════════════════════════════════════════════════════╝"
    echo ""
    fi
    ```
3. e2e-linux.sh — add --vnc mode
   New flag `--vnc` that:

    1. Reuses existing Docker image build logic (rebuilds if needed)
    2. Does NOT require a pre-built binary (uses pnpm dev instead)
    3. Starts the container with:
        - -it (interactive, for Ctrl+C)
        - -p 5990:5990 -p 6090:6090 (VNC ports)
        - VNC=1 env var (triggers VNC in entrypoint)
        - TAURI_DEV_HOST=0.0.0.0 (so Vite binds to all interfaces for HMR inside container)
        - RUST_LOG=debug (verbose backend logs)
        - Backs up .cargo/config.toml (same pattern as build mode, for local path patches)
        - Runs: pnpm install --frozen-lockfile && pnpm dev
    4. Source code is mounted read-write, so host edits trigger Vite HMR
    5. node_modules are Docker volumes (same as existing modes, prevents cross-platform contamination)
4. package.json — convenience script
   ```
   "test:e2e:linux:vnc": "./scripts/e2e-linux.sh --vnc"
   ```

5. Docs — add section to e2e-testing-guide.md
    Short section under "Interactive debugging" explaining the VNC mode, with the one-liner and a screenshot workflow.

## How it works end-to-end

Terminal: `pnpm test:e2e:linux:vnc`
↓ builds Docker image (first run only)
↓ starts container with VNC + pnpm dev
↓ streams Rust/Vite logs to stdout

Browser: http://localhost:6090/vnc.html?autoconnect=true
↓ shows Cmdr running in Linux GTK
↓ user clicks, presses F5/F7, observes behavior

Editor: edit `.svelte/.ts` files on macOS
↓ Vite HMR picks up changes instantly (~1–3 sec)
↓ Cmdr app in VNC reloads automatically

Rust changes require restart (Ctrl+C + re-run). Frontend changes are instant.

Verification

1. Run pnpm test:e2e:linux:vnc
2. Wait for "Open in browser" message
3. Open the URL — see Cmdr app
4. Press F7 — does the new folder dialog open?
5. Press F5 — does the copy dialog open?
6. Edit a .svelte file — does the app hot reload?
7. Check terminal for debug logs
8. Ctrl+C stops everything cleanly
