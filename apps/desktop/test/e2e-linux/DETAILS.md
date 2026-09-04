# Linux E2E Docker infrastructure details

`CLAUDE.md` holds the must-knows. This file holds the build-caching mechanics, the SMB networking detail, the full
webkit2gtk caret investigation, the Playwright-on-26.04 workaround detail, and the Ubuntu test VM playbook.

## Scope: single shard, no per-instance fixture isolation

Linux Docker runs a single shard, so the per-instance fixture isolation built for parallel macOS shards
(`/tmp/cmdr-e2e-fixtures-<instance>-<ts>/` plus a hardlink cache at `/tmp/cmdr-e2e-fixtures-cache/`) is out of scope.
`e2e-linux.sh` calls `createFixtures()` without an instance ID, falling back to the legacy shared `/tmp/cmdr-e2e-<ts>/`
root with no cache. The 170 MB regen cost is paid once per run and is dominated by container startup; the bookkeeping
for one shard isn't worth it.

## Architecture

```
e2e-linux.sh
├─ Build Tauri binary in Docker (--features playwright-e2e,virtual-mtp,smb-e2e)
├─ Start SMB Docker containers (smb-consumer-guest, -auth, -50shares, -unicode)
├─ Launch E2E container on smb-consumer_default network
│   ├─ entrypoint.sh: Xvfb + dbus + GVFS + optional VNC
│   ├─ Create fixtures, start Tauri app (with SMB_E2E_*_HOST/PORT env vars)
│   ├─ Wait for /tmp/tauri-playwright.sock
│   └─ Run: npx playwright test --config test/e2e-playwright/playwright.config.ts
└─ Report results
```

Files:

- `docker/Dockerfile.base`: Ubuntu 26.04 system layer (Tauri prereqs, Xvfb, Rust, Node, Playwright chromium libs,
  patchelf).
- `docker/Dockerfile`: thin final layer (`FROM cmdr-e2e-base:<hash>` + entrypoint).
- `docker/entrypoint.sh`: Xvfb/dbus/GVFS/VNC setup for headless GUI, plus the Playwright host-platform override.

## Build caching

### Docker images: content-addressed base + thin final layer

The image is split in two so per-run builds never re-install the slow system layer.

- **Base image** (`docker/Dockerfile.base`): apt packages (Tauri prereqs, GVFS, Playwright chromium libs, patchelf),
  Node, pnpm, Rust. `e2e-linux.sh` tags it `cmdr-e2e-base:<first 12 hex of sha256(Dockerfile.base)>` (plus `latest`), so
  invalidation is automatic: editing `Dockerfile.base` changes the hash, the tag is missing, and the next run rebuilds.
  An unchanged file reuses the existing image with zero build work. After building a new base, the script prunes older
  `cmdr-e2e-base` tags (~3.5 GB each).
- **Final image** (`docker/Dockerfile`): `FROM <base>` + `entrypoint.sh`. Rebuilt every run (~1 s with layer cache), so
  `entrypoint.sh` and `Dockerfile` edits propagate automatically (no `--build` needed).

Force-rebuilding the base (`pnpm test:e2e:linux:build` or `./scripts/e2e-linux.sh --build`) rebuilds with `--no-cache`.
Only needed to refresh content baked into cached layers (stale apt lists, a newer rustup or Node point release); file
edits invalidate on their own. `--build-only` builds the images and exits without running tests. The dev packages the
build needs (`libwebkit2gtk-4.1-dev`, `patchelf`, friends) are baked into the base.

### CI base-image persistence

In the `ci.yml` `desktop-e2e-linux` job, runners start fresh, so the base image is persisted as a `docker save` tar via
`actions/cache`, keyed on `hashFiles(Dockerfile.base)` (same invalidation signal as the local tag). Cache hit →
`docker load` (then the tar is deleted to reclaim ~3.5 GB; a hit is always exact, no `restore-keys`). Cache miss → the
script builds the base, and an `Export E2E base image` step re-creates the tar for the cache post-step. No registry
involved.

### Volumes (cargo, target, node_modules, .svelte-kit)

One volume is global, because it's content-addressed and expensive to refill:

- `cmdr-cargo-cache` → `/root/.cargo/registry`: the downloaded crate index and `.crate` sources (NOT compiled
  artifacts). Remove to force a full crate re-download.

The other four are **per checkout**, suffixed `-<dir name>-<8 hex of the checkout's absolute path>` (so
`cmdr-target-cache-transfer-fixes-25dacb0e` for a worktree at `.claude/worktrees/transfer-fixes`):

- `cmdr-target-cache-<key>` → `/target`: the whole Linux `CARGO_TARGET_DIR`, ~4.6 GB fresh. Remove to force a full app
  recompile.
- `cmdr-root-node-modules-cache-<key>` → `/app/node_modules`: root `node_modules/`. Remove to force `pnpm install`.
- `cmdr-desktop-node-modules-cache-<key>` → `/app/apps/desktop/node_modules`: same for the desktop package.
- `cmdr-desktop-sveltekit-cache-<key>` → `/app/apps/desktop/.svelte-kit`: the frontend build state, including the
  container-private adapter output (`CMDR_FRONTEND_BUILD_DIR`).

`./scripts/e2e-linux.sh --clean` drops exactly this checkout's four; it leaves the shared registry and every other
checkout alone.

**Why they're keyed, and what it costs.** All four hold state built from the sources bind-mounted at `/app`, and `/app`
is whichever clone or worktree launched the script. Under one shared set the container paths are identical across
checkouts, so cargo compares your source mtimes against an rlib another worktree built, calls it fresh, and links a
`cmdr-fs` from someone else's branch. The tell is a build failure naming API your branch legitimately has
(`no volume_busy_for_user in volume::host::activity`) with `Compiling cmdr` as the ONLY compile line. `node_modules` and
`.svelte-kit` are the same class of state, and two checkouts running at once would race inside them. Keying on the path
rather than the branch is deliberate: the bind mount is what the cache belongs to, so a branch rename or a second
worktree on the same branch doesn't move a cache.

**What the split costs, measured** (M4 Max, 2026-09-02, `pnpm check desktop-e2e-linux` wall clock; the shared cargo
registry was warm throughout, and the 320-test run dominates every number): 8m55s on a cold target volume, 8m15s with
one Rust file edited, 7m13s with nothing to rebuild. So a new checkout pays about **1m40s once**, not a fresh
workspace's worth of compiling, and a fresh volume weighs ~4.6 GB. Worth knowing before optimizing this: the single
shared volume it replaced had drifted to 17.4 GB of artifacts from branches long gone, so per-checkout volumes plus the
reaper below use LESS disk in practice, not more.

**Stale volumes reap themselves.** The script labels the four with `com.cmdr.e2e-linux-cache=1` and
`com.cmdr.e2e-linux-checkout=<abs path>`, and every run removes labelled volumes whose checkout path is gone from disk;
in-use volumes refuse removal, so a concurrent run is safe. Labelling happens BEFORE any `docker run -v`, which matters:
`-v` also creates a missing volume but without labels, and `docker volume create` won't add labels to a volume that
already exists (verified on Docker 29.4.0, 2026-09-02), so a volume born from `-v` would be invisible to the reaper
forever.

`CARGO_VOLUME`, `TARGET_VOLUME`, `ROOT_NODE_MODULES_VOLUME`, `DESKTOP_NODE_MODULES_VOLUME`, and
`DESKTOP_SVELTEKIT_VOLUME` each override the name. An override is taken verbatim: no suffix, no label, no reaping. CI
sets the first two to host bind-mount paths (`/tmp/cmdr-docker-cache/...`) so `actions/cache` can persist them (it can't
cache Docker named volumes), and a CI runner has one checkout anyway.

## SMB E2E networking

The E2E container joins the `smb-consumer_default` Docker network so it can reach the four SMB containers by name on
:445: `smb-consumer-guest` (no-auth public share), `smb-consumer-auth` (testuser/testpass), `smb-consumer-50shares` (a
server exposing 50 shares, for share-list scale), and `smb-consumer-unicode` (shares with CJK, emoji, and accented
names). They come from smb2's consumer test harness; `e2e-linux.sh` starts exactly this set (via
`smb-servers/start.sh e2e`) and passes per-container env vars (`SMB_E2E_GUEST_HOST` / `_PORT`, plus the `AUTH` /
`50SHARES` / `UNICODE` equivalents) to the Tauri app, which `virtual_smb_hosts.rs` reads to inject the correct
addresses. On macOS (local dev), smb2's default ports (guest 10480, auth 10481, 50shares 10483, unicode 10484) are used
instead.

The Docker image includes `smbclient` (for the `smb_smbclient.rs` fallback), `cifs-utils`, and GVFS packages (`gvfs`,
`gvfs-backends`, `gvfs-daemons`, `gvfs-fuse`). The entrypoint starts `gvfsd` so `gio mount` works for user-space SMB
mounting (what Cmdr's `mount_linux.rs` uses). Pre-mounting via `gio mount` produces the same GVFS paths the app expects
(`/run/user/<uid>/gvfs/smb-share:server=<host>,share=<share>`). The container runs `--privileged` because Docker's
default seccomp blocks the `mount` syscall even with `CAP_SYS_ADMIN`, and GVFS-FUSE needs `/dev/fuse`.

The SMB probe (`probe_smb_ports`) runs per-service TCP probes on :445 on both fresh-start and already-running paths,
because Docker reporting a container as `running` only means it's alive (smbd can be hung, OOM-killed, or still
initialising). A previous version trusted the running-check and skipped the probe, producing
`Cannot reach smb-consumer-X` failures whenever a stale stack from a prior run was unhealthy. The final probe (30 s
deadline) emits `SMB e2e stack ready: all 4 containers accepting TCP on :445`. A post-flight probe (after tests,
`set +e`-wrapped so it can't mask the test exit code) emits either
`SMB post-flight: all 4 containers still accepting TCP on :445` or
`...at least one container is no longer accepting TCP, likely died mid-run` plus per-service compose state. Both banners
are hoisted to the top of the failing-test summary (prefixed `[SMB]`): pre-OK + post-FAIL localises to "died mid-run",
both OK to Cmdr-side SMB code, both FAIL to infra / Docker networking.

## webkit2gtk caret bug (why the base is `ubuntu:26.04`)

webkit2gtk 2.50.4 (ships with Ubuntu 24.04) returns `startOffset: 0` from `document.caretRangeFromPoint(x, y)` for ALL
x-coordinates inside text whose ancestor chain has `user-select: none`. It's what put this base image on 26.04:
`viewer.spec.ts` "drag within viewport selects the dragged range" got an empty selection, `runCopy()` returned
`{ kind: 'empty' }`, no toast appeared, the test timed out.

The production caret path doesn't touch that API. `routes/viewer/viewer-pointer.ts` hit-tests the rendered rows and
binary-searches `Range.getClientRects()` boxes; the reasoning is in `apps/desktop/src/routes/viewer/DETAILS.md` §
"Pointer → caret". So this bug alone doesn't decide the base image any more, and the pin is a plain "26.04 is what the
suite is green on". ❌ Still don't move it casually: the rest of the stack (Playwright's platform-registry workarounds
below, the apt package set) is tuned to 26.04, and only a full suite run can tell you an older base is fine.

How we know it's this: probed at x = 25, 35, 50, 75, 200, 500 on `.line-text` (`user-select: none`); all return offset 0
on 2.50.4, but offset 1, 4, 7, 25, 68 (monotonic per-character) on 2.52.3 on the same Xvfb display server. Control:
probing the status-bar text (`user-select: text`) returns sensible offsets even on 2.50.4. So it's a `user-select: none`

- webkit2gtk 2.50.4 interaction, not Xvfb itself. Ruled out: `WEBKIT_DISABLE_COMPOSITING_MODE=1`,
  `WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1` (none help); fonts (DejaVu Sans Mono present
  in both); GDK backend (Xwayland and Xvfb both work with 2.52.3). Real Linux users (Wayland or X11 with a real display
  server) were never affected, only the synthetic-pointer-event test path.

Kept because it's cheap evidence: if a future caret regression shows up only on Linux, this probe (offsets on
`user-select: none` text vs. the `user-select: text` status bar) is how you tell a webview bug from ours.

## Playwright on the 26.04 base image (and bumping Playwright)

26.04 is newer than Playwright knows about. Playwright 1.59's bundled platform registry
(`node_modules/playwright-core/.../server/registry/index.js`) only lists `ubuntu20.04` / `22.04` / `24.04` (each with
`-x64` and `-arm64`). On 26.04, an unguarded `playwright install chromium` fails the platform check with
`Error: Playwright does not support chromium on ubuntu26.04`. Two workarounds, both must stay in sync on a Playwright
bump:

1. **Host-platform override (`entrypoint.sh`).** Exports `PLAYWRIGHT_HOST_PLATFORM_OVERRIDE` so Playwright downloads the
   24.04 fallback build (libc-compatible, runs fine on 26.04). It MUST carry the arch suffix and match the runtime arch:
   `ubuntu24.04-arm64` on Apple Silicon (local), `ubuntu24.04-x64` on x86_64 CI. A bare `ubuntu24.04` matches no
   registry key and reproduces the "does not support chromium" error. A local arm64 run passes and masks an amd64-only
   break: reproduce override changes under `docker run --platform linux/amd64` before trusting them.
2. **Chromium runtime libs (`Dockerfile.base`).** We run `playwright install chromium` (binary only), NOT `--with-deps`:
   the `--with-deps` apt step re-derives the distro from `/etc/os-release` (sees 26.04, has no dep list, fails)
   regardless of the override. So the chromium runtime libs `--with-deps` would install (`libnss3`, `libnspr4`,
   `libgbm1`, `libdrm2`, `libcups2t64`, `libxkbcommon0`, `libatspi2.0-0`, `libatk-bridge2.0-0`, `libasound2t64`,
   `libxcb1`) are apt-installed explicitly in `Dockerfile.base`, where plain apt on 26.04 has no Playwright version
   gate. Keep that list in sync with Playwright's `registry/nativeDeps.js` (`ubuntu24.04-x64` chromium deps) on a bump.

On a Playwright bump: first check whether the new registry natively lists `ubuntu26.04`. If yes, delete both workarounds
(the `entrypoint.sh` override block and the chromium-libs stanza) and switch to plain
`playwright install --with-deps chromium`. If no, re-confirm the override arch tags still match a registry key and
re-sync the `Dockerfile.base` lib list against the new `nativeDeps.js`.

## Ubuntu test VM (faster than CI roundtrips)

David runs a UTM-Apple-Virtualization Ubuntu VM that mounts the repo via VirtioFS. File edits on the Mac side are
visible inside the VM immediately, so iterating on Linux-only test code (`accent_color_linux`, `volume::mtp_linux`, GVFS
/ D-Bus paths) is much faster than pushing to CI. Setup is in `CONTRIBUTING.md` § "Linux testing (Ubuntu VM)". The host
is on DHCP, so the IP rotates; ask the user, or run `ip -4 addr show enp0s1 | awk '/inet /{print $2}'` inside the VM.
User is `veszelovszki`; SSH is key-based.

Iterate loop from the Mac:

```bash
VM=10.139.81.203                    # replace with current IP
ssh veszelovszki@$VM 'bash -lc "cd ~/cmdr/apps/desktop/src-tauri && cargo nextest run --no-fail-fast <test-name>"'
```

The repo is bind-mounted at `~/cmdr` (= `/mnt/cmdr/cmdr`). `target/` and `node_modules/` are intentionally on the VM's
local disk, not VirtioFS (virtiofs is slow for the millions of small files cargo and pnpm produce). That local disk is
small (62 GB), so on `No space left on device`:

```bash
ssh veszelovszki@$VM 'df -h /mnt/cmdr/cmdr/target'                  # check usage
ssh veszelovszki@$VM 'rm -rf /mnt/cmdr/cmdr/target/release \
                            /mnt/cmdr/cmdr/target/debug/incremental \
                            /mnt/cmdr/cmdr/target/debug/Cmdr'      # free 10 GB in seconds
```

A full `cargo clean` works but loses all dep artifacts (next build pays ~10 min on aarch64). Prefer the targeted
cleanup.

Gotchas:

- On first SSH after the VM reboots, `rustup` may re-sync the toolchain (~1 min) before `cargo` returns. Wrap the first
  test invocation in a high timeout.
- The VM has a half-configured D-Bus (session-bus socket present, daemon serving), exactly the shape that broke
  `read_accent_color_returns_valid_hex` in CI for 4 days. Linux unit tests with bounded probes can exercise the live
  D-Bus path here, in addition to Docker (where the bus is absent entirely). That dual coverage is the reason to keep
  the VM for unit tests, not just E2E.
