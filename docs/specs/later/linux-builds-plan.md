# Add Linux build target

## Context

Cmdr already has Linux Rust dependencies (`zbus`, `freedesktop-icons`, `libacl`, etc.) and the CI runs Linux E2E tests. The missing piece is a release build that produces distributable artifacts and a website that detects Linux visitors and offers them the right download.

**Goal:** When a Linux user visits getcmdr.com, they see a Linux download button (AppImage by default, .deb as alternative). macOS users continue to see exactly what they see today. The updater works for both platforms.

**Architecture, not distro.** Linux distro detection is unnecessary. AppImage works everywhere. The only axis that matters is CPU architecture. We build for both `x86_64` and `aarch64` from day one — aarch64 is useful for ARM VMs and growing ARM Linux desktop/server usage.

## Part 1: CI workflow

**File: `.github/workflows/release.yml`**

Add two Linux matrix entries to the existing `build` job:

```yaml
- target: x86_64-unknown-linux-gnu
  arch: x86_64
  os: ubuntu-latest
  platform: linux
- target: aarch64-unknown-linux-gnu
  arch: aarch64
  os: ubuntu-24.04-arm
  platform: linux
```

The existing macOS entries get `os: macos-latest` and `platform: macos` to distinguish them.

The aarch64 build runs on GitHub's ARM runner (`ubuntu-24.04-arm`) for native compilation — no cross-compilation needed. Native is simpler and faster than cross-compilation.

### Linux build steps

The Linux matrix entry needs different setup than macOS (no signing/notarization, different system deps):

1. **System dependencies** — same as CI already uses:
   ```bash
   sudo apt-get update
   sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libacl1-dev
   ```
2. **Checkout, mise, pnpm install, svelte-kit sync** — identical to macOS.
3. **No Apple certificate / notarization steps** — skip via `if: matrix.platform == 'macos'` conditions on those steps.
4. **Build with tauri-action** — same as macOS entries, just `--target ${{ matrix.target }}`. Override `bundle.targets` to `["deb", "appimage"]` for Linux (via `--bundles deb,appimage` or Tauri config override) to avoid attempting RPM builds that need `rpmbuild`.
5. **Upload artifacts** — find the `.AppImage`, `.deb`, and updater `.AppImage.tar.zst` + `.sig` in `target/${{ matrix.target }}/release/bundle/`. Upload to the GitHub release with `gh release upload --clobber`. Rename updater artifact to include platform + arch: `Cmdr_linux_x86_64.AppImage.tar.zst` / `Cmdr_linux_aarch64.AppImage.tar.zst`.
6. **Pass signature to publish job** — same pattern as macOS: `actions/upload-artifact` with the `.sig` content.

### Publish job changes

The `publish` job downloads signatures from all matrix jobs (now 5: three macOS + two Linux). Generate `latest.json` with the additional platform keys:

```json
{
  "platforms": {
    "darwin-universal": { ... },
    "darwin-aarch64":   { ... },
    "darwin-x86_64":    { ... },
    "linux-x86_64":     { "url": ".../Cmdr_linux_x86_64.AppImage.tar.zst", "signature": "..." },
    "linux-aarch64":    { "url": ".../Cmdr_linux_aarch64.AppImage.tar.zst", "signature": "..." }
  }
}
```

### Artifact naming

| Type | x86_64 pattern | aarch64 pattern |
|------|----------------|-----------------|
| AppImage | `Cmdr_${version}_amd64.AppImage` | `Cmdr_${version}_arm64.AppImage` |
| Deb | `cmdr_${version}_amd64.deb` | `cmdr_${version}_arm64.deb` |
| Updater | `Cmdr_linux_x86_64.AppImage.tar.zst` | `Cmdr_linux_aarch64.AppImage.tar.zst` |

Tauri uses Debian-style arch names (`amd64`/`arm64`) for AppImage and .deb filenames. The updater artifacts use our own naming with `linux_x86_64`/`linux_aarch64` for consistency with the macOS pattern. Check Tauri's actual output filenames during a test build — they may differ slightly.

### Bundled resources: llama-server

`download-llama-server.go` already creates an empty placeholder on non-macOS. The AI feature will be macOS-only initially, so no changes needed. The placeholder file satisfies Tauri's resource requirement.

## Part 2: Tauri config

**File: `apps/desktop/src-tauri/tauri.conf.json`**

The existing config mostly works for Linux as-is:
- `bundle.macOS` section is ignored on Linux.
- `bundle.icon` includes PNG files that Linux uses.

Changes needed:
- **Bundle targets**: Change from `"all"` to explicitly listing targets per platform, or override in CI. On Linux we only want `deb` and `appimage` (not RPM). The CI build step should pass `--bundles deb,appimage` to avoid needing `rpmbuild`.
- **Add `bundle.linux` section**: Include a `.desktop` file with proper `Categories` (for example, `Utility;FileManager;`), `MimeType`, and icon references. Without this, the app may not show up correctly in GNOME/KDE app launchers. Tauri generates a minimal default, but it's worth getting right from day one since it's almost no effort.

## Part 3: Website

### 3a. `release.ts` — add Linux URLs and sizes

```ts
export const appImageUrls = {
  x86_64: `${base}/Cmdr_${version}_amd64.AppImage`,
  aarch64: `${base}/Cmdr_${version}_arm64.AppImage`,
}
export const debUrls = {
  x86_64: `${base}/cmdr_${version}_amd64.deb`,
  aarch64: `${base}/cmdr_${version}_arm64.deb`,
}
```

Add `appImageSizes` alongside `dmgSizes`, reading from `latest.json`:
```ts
export const appImageSizes =
  rawSizes.appImageSizes.x86_64 > 0
    ? {
        x86_64: formatBytes(rawSizes.appImageSizes.x86_64),
        aarch64: formatBytes(rawSizes.appImageSizes.aarch64),
      }
    : null
```

Extend `latest.json` schema: add `appImageSizes: { x86_64: 0, aarch64: 0 }` next to `dmgSizes` so the publish job can populate it.

### 3b. `Layout.astro` — extend the inline script for OS detection

The existing script detects macOS architecture via `userAgentData`. Extend it to also detect the OS so download links can swap between macOS and Linux.

OS detection fallback chain (from most to least reliable):

1. `navigator.userAgentData.platform` — modern Chromium browsers, returns `"Linux"` directly.
2. `navigator.platform` — deprecated but still widely supported, returns `"Linux x86_64"` or similar.
3. `navigator.userAgent` regex — last resort for browsers where the above return empty, check for `/Linux/`.

```js
var isLinux = (navigator.userAgentData && navigator.userAgentData.platform === 'Linux')
  || /Linux/.test(navigator.platform)
  || /Linux/.test(navigator.userAgent)
```

When Linux is detected:
1. Set `data-os="linux"` on `<html>`.
2. Determine Linux arch: use `navigator.userAgentData.getHighEntropyValues(['architecture'])` if available (Chromium), otherwise default to `x86_64`.
3. Swap `href` on all `<a data-download-link>` elements: read from `data-linux-appimage-x86_64` or `data-linux-appimage-aarch64` attribute based on detected arch.
4. Update `data-download-size` text if available.
5. The Download.astro component uses `data-os` to toggle visibility of platform-specific sections (see below).

When macOS is detected (or no detection):
- Everything stays exactly as it is today. No behavioral change for macOS users.

### 3c. Download link components — add Linux data attributes

All download `<a>` tags that currently have `data-download-link` get one more attribute:

```html
<a href={dmgUrls.universal}
   data-download-link
   data-dmg-arm={dmgUrls.aarch64}
   data-dmg-intel={dmgUrls.x86_64}
   data-linux-appimage-x86_64={appImageUrls.x86_64}
   data-linux-appimage-aarch64={appImageUrls.aarch64}
   ...>
```

**Files to update:**
- `Hero.astro` — CTA button: add `data-linux-appimage`. JS swaps href + button text ("Download for Linux").
- `Header.astro` — desktop + mobile download buttons: same pattern.
- `pricing.astro` — download button: same pattern. Update the "macOS only" subtitle to show "Linux" when detected.
- `Download.astro` — full platform card (see below).

### 3d. `Download.astro` — platform-aware download card

Replace the single macOS card with a structure that adapts based on detected OS. Static HTML defaults to macOS (the primary platform). JS shows the Linux variant for Linux visitors.

**Layout:**

The current download card shows:
- Apple logo + "macOS" + arch subtitle
- Big download button
- "Also available: Apple Silicon | Intel | Universal"

For Linux visitors, the same card structure swaps to:
- Tux/Linux logo + "Linux" + arch subtitle (detected or "x86_64" default)
- Big download button for AppImage (arch-matched)
- "Also available: aarch64 | x86_64 | .deb (x86_64) | .deb (aarch64)"
- Below: "Also available for macOS" link (small, secondary)

For macOS visitors (no change from today except one addition):
- Everything as-is
- Below the arch links: "Also available for Linux" link (small, secondary)

**Implementation approach:**

Two card variants inside `Download.astro`, toggled by CSS using `html[data-os="linux"]`:
- `data-platform-card="macos"` — shown by default, hidden when `data-os="linux"`
- `data-platform-card="linux"` — hidden by default, shown when `data-os="linux"`

This avoids a flash of the wrong platform since the macOS card renders instantly and the Linux card swaps in after JS runs. The swap is fast enough to be imperceptible.

The "Windows coming soon" newsletter CTA at the bottom of Download.astro changes to "Windows coming soon" (just drop "Linux" from the text).

### 3e. No-JS / unsupported browser behavior

All links default to macOS (universal DMG) in static HTML. Linux users without JS or with unusual browsers see the macOS download — but they're technical enough to find the GitHub releases page. The "Also available for Linux" link is visible in the HTML regardless of JS.

## Part 4: Updater compatibility

No Tauri code changes needed. The updater reads `latest.json` and selects the platform key matching its compiled target:
- Linux installs compiled with `x86_64-unknown-linux-gnu` match `linux-x86_64`
- Linux installs compiled with `aarch64-unknown-linux-gnu` match `linux-aarch64`

**Important**: During the first test build (Milestone 3), verify that the platform key Tauri's updater actually sends matches exactly what we put in `latest.json`. A mismatch means silent updater failure — the app would appear up-to-date when it isn't. Check Tauri's updater source or logs to confirm the target string format.

## Part 5: `latest.json` schema update

Current:
```json
{
  "version": "...",
  "dmgSizes": { "aarch64": 0, "x86_64": 0, "universal": 0 },
  "platforms": { ... }
}
```

After:
```json
{
  "version": "...",
  "dmgSizes": { "aarch64": 0, "x86_64": 0, "universal": 0 },
  "appImageSizes": { "x86_64": 0, "aarch64": 0 },
  "platforms": {
    "darwin-universal": { ... },
    "darwin-aarch64": { ... },
    "darwin-x86_64": { ... },
    "linux-x86_64": { ... },
    "linux-aarch64": { ... }
  }
}
```

The publish job populates `appImageSizes` by checking AppImage file sizes, same as it does for DMGs.

## Part 6: Rust build confidence

The codebase already compiles for Linux — the CI runs Linux E2E tests via `cargo build --target x86_64-unknown-linux-gnu` on every PR. All platform-specific Rust code uses `#[cfg(target_os = "...")]` gates (`zbus`, `freedesktop-icons`, `libacl`, etc. for Linux; `objc2`, `core-foundation`, etc. for macOS). The `cfg-gate` check in CI (`./scripts/check.sh --check cfg-gate`) verifies that no macOS-only code leaks into Linux builds and vice versa. No additional Rust changes are expected for the release build — the same binary that passes E2E tests is the one that gets bundled.

## Verification

1. **CI**: Push a test tag (for example, `v0.6.0-rc.1`) and verify:
   - Two AppImages and two .debs (x86_64 + aarch64) appear on the GitHub release alongside the three DMGs
   - `latest.json` has both `linux-x86_64` and `linux-aarch64` platform entries with valid signatures
   - Both updater artifacts are present
2. **Website**: `pnpm dev` in `apps/website/`, override `navigator.platform` in DevTools to `Linux x86_64`:
   - Download buttons swap to AppImage links
   - Download.astro shows the Linux card with x86_64 as default
   - The "Also available for macOS" link is visible
   - Switch back to macOS UA — verify macOS card shows as before, with "Also available for Linux" link
3. **Updater**: Install from the AppImage on a Linux machine (or VM), verify it picks up the correct `linux-*` entry from `latest.json`. Confirm the platform key the Tauri updater requests matches exactly what `latest.json` provides (check updater logs)
4. **Smoke test**: Download the x86_64 AppImage on Ubuntu (and aarch64 on an ARM VM), `chmod +x`, run it, verify the app launches

## Tasks

### Milestone 1: CI builds and Tauri config
- [ ] Add `bundle.linux` section to `tauri.conf.json` with `.desktop` categories (`Utility;FileManager;`) and icon refs
- [ ] Add Linux x86_64 and aarch64 matrix entries to `release.yml` build job (use `--bundles deb,appimage`)
- [ ] Conditionalize macOS-only steps (certificate, notarization) with `if: matrix.platform == 'macos'`
- [ ] Add Linux system deps install step with `if: matrix.platform == 'linux'`
- [ ] Upload Linux artifacts (AppImage, .deb, updater) for both archs to GitHub release
- [ ] Pass Linux updater signatures to publish job
- [ ] Extend publish job to include `linux-x86_64` and `linux-aarch64` in `latest.json`
- [ ] Add `appImageSizes` to `latest.json` schema and populate from AppImage file sizes

### Milestone 2: Website
- [ ] Add `appImageUrls`, `debUrls`, and `appImageSizes` exports to `release.ts`
- [ ] Extend `Layout.astro` inline script: detect Linux via `userAgentData.platform` → `navigator.platform` → `userAgent` fallback chain, detect arch, set `data-os` on `<html>`, swap download links
- [ ] Add `data-linux-appimage-*` attributes to download links in Hero, Header, pricing
- [ ] Build platform-aware Download.astro with macOS/Linux card toggle and arch selector for Linux
- [ ] Add "Also available for Linux/macOS" cross-platform links
- [ ] Update "Windows and Linux coming soon" text to "Windows coming soon"
- [ ] Run website checks: `./scripts/check.sh --check website-prettier,website-eslint,website-typecheck,website-build,website-e2e`

### Milestone 3: Verification
- [ ] Test with a release candidate tag
- [ ] Verify all artifacts (x86_64 + aarch64) on GitHub release
- [ ] Verify `latest.json` has correct Linux entries for both archs
- [ ] Verify updater platform keys match exactly between Tauri's request and `latest.json` entries
- [ ] Verify website OS detection in Chrome DevTools
- [ ] Smoke-test x86_64 AppImage on Ubuntu
- [ ] Smoke-test aarch64 AppImage on ARM VM
