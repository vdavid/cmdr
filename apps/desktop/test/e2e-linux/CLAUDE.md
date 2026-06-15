# Linux E2E Docker infrastructure

Docker setup for running the Playwright E2E tests on Linux. The test files live in `../e2e-playwright/` and are shared
between macOS and Linux; see `e2e-playwright/CLAUDE.md` for test docs. `e2e-linux.sh` builds the Tauri binary in Docker,
starts the SMB containers, launches the E2E container, and runs `npx playwright test`. Architecture diagram in
DETAILS.md.

## Running

```bash
cd apps/desktop
pnpm test:e2e:linux                    # Full run: build (if needed) + test in Docker
pnpm test:e2e:linux:build              # Force-rebuild Docker images (base with --no-cache), no tests
pnpm test:e2e:linux:shell              # Interactive shell in container
pnpm test:e2e:linux:vnc                # VNC mode with hot reload (pnpm dev)
./scripts/e2e-linux.sh --grep "SMB"    # Run only tests matching a pattern
```

## Must-knows

- **Don't add `apt-get` to the `docker run` blocks in `e2e-linux.sh`.** Per-run containers run no apt at all; all dev
  packages are baked into `docker/Dockerfile.base`. Put new packages there. The base is content-addressed (tagged by
  `sha256(Dockerfile.base)`) so editing it auto-invalidates; the thin final `docker/Dockerfile` rebuilds every run, so
  `entrypoint.sh` / `Dockerfile` edits propagate with no `--build`. Build-caching detail in DETAILS.md.
- **The base image is pinned to `ubuntu:26.04` (not 24.04) on purpose.** webkit2gtk 2.50.4 (ships with 24.04) has a bug
  where `document.caretRangeFromPoint` returns `startOffset: 0` inside `user-select: none` text, breaking the viewer's
  pointer-drag selection (`viewer.spec.ts`). To drop back you'd skip that test or rework production caret-from-point.
  Full evidence in DETAILS.md.
- **Don't "optimize away" the chromium install** in `Dockerfile.base`. No test drives a browser (every spec runs the
  `tauri` socket-bridge project), but `@playwright/test` still launches a headless chromium per worker as a runtime
  dependency; remove it and all ~196 tests fail at setup with `Executable doesn't exist`.
- **Two Playwright-on-26.04 workarounds must stay in sync on every Playwright bump** (26.04 is newer than Playwright's
  platform registry knows): the `PLAYWRIGHT_HOST_PLATFORM_OVERRIDE` arch tag in `entrypoint.sh`, and the chromium
  runtime libs apt-installed in `Dockerfile.base` (we run `playwright install chromium`, NOT `--with-deps`, which fails
  on 26.04). A local arm64 run masks amd64-only breaks: reproduce override changes under `--platform linux/amd64`. Full
  recipe in DETAILS.md.
- **SMB container readiness is always actively probed** (`e2e-linux.sh:probe_smb_ports`, per-service TCP probe on :445)
  on both fresh-start and already-running paths, because Docker reporting `running` doesn't mean smbd has bound the
  port. A failed already-running probe tears down and restarts the stack; a post-flight probe runs after tests. Both
  banners are hoisted into the failing-test summary (prefixed `[SMB]`). Enforces the no-magic-sleep rule
  (`apps/desktop/test/CLAUDE.md`).
- **Volume name gotcha**: root is "Root" on Linux, "Macintosh HD" on macOS. Tests that emit `mcp-volume-select` to
  switch to a local volume use `LOCAL_VOLUME_NAME` constants (`smb.spec.ts`, `mtp.spec.ts`).
- **`mcp-volume-select` listener exists only on the file explorer route (`/`), not `/settings`.** A `beforeEach` must
  navigate to `/` before emitting volume-select events, or they're silently ignored.
- **GVFS needs the D-Bus session bus and `gvfsd` running before any `gio mount`.** The entrypoint starts `dbus-launch`
  then `/usr/libexec/gvfsd` in that order; `XDG_RUNTIME_DIR` must be `/run/user/<uid>` (not `/tmp/...`) for mount paths
  to match `mount_linux.rs`'s `derive_gvfs_path`. The container runs `--privileged` (default seccomp blocks `mount` even
  with `CAP_SYS_ADMIN`, and GVFS-FUSE needs `/dev/fuse`).
- **Run SMB tests in isolation.** In sequence, the app can exit before SMB tests (the accessibility test walks MCP
  settings, which with cross-window setting sync can trigger an MCP state change). Root cause tracked separately.

## CI integration

- `desktop-e2e-linux`: Playwright E2E in Docker. Not included by default (slow).
- `e2e-linux-typecheck`: TypeScript check on e2e-playwright files. Included by default.

The base-image tar persistence and build-volume bind-mounts are in DETAILS.md § Build caching.

## Ubuntu test VM

David runs a UTM Ubuntu VM (repo mounted via VirtioFS) for fast iteration on Linux-only test code, faster than CI
roundtrips. Setup, the SSH iterate loop, disk-cleanup recipes, and the half-configured-D-Bus gotcha are in DETAILS.md §
Ubuntu test VM.

Full details: [DETAILS.md](DETAILS.md).
