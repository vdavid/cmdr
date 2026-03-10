# Split macOS builds into arch-specific downloads

## Context

The universal DMG is ~30 MB, almost all native code. Splitting into arch-specific builds (aarch64 / x86_64) cuts download size roughly in half (~15 MB). This also prepares for Homebrew Cask distribution, which needs arch-specific artifacts.

## Part 1: CI workflow

**File: `.github/workflows/release.yml`**

Split the single `release` job into two jobs:

### Job 1: `build` (matrix, 3 parallel runners)

```yaml
strategy:
  fail-fast: false
  matrix:
    include:
      - target: aarch64-apple-darwin
        arch: aarch64
      - target: x86_64-apple-darwin
        arch: x86_64
      - target: universal-apple-darwin
        arch: universal
```

Each matrix entry runs on `macos-latest` and does:

1. Checkout, mise, pnpm install, svelte-kit sync — same as today
2. **Rustup targets** — conditionally add `x86_64-apple-darwin` only for the `x86_64` and `universal` jobs (aarch64 is native on the ARM runner)
3. Import Apple certificate, set up notarization — identical to today
4. **Build with tauri-action** — but **without** `tagName`/`releaseName` so it doesn't try to create/upload to a release. Just build + sign + notarize:
   ```yaml
   with:
     projectPath: ./apps/desktop
     args: --target ${{ matrix.target }}
   ```
5. **Upload artifacts manually** — find the DMG and updater `.app.tar.zst` + `.sig` in `target/{target}/release/bundle`, rename updater files to include arch (`Cmdr_aarch64.app.tar.zst`, etc.), create the release if needed (`gh release create ... || true`), upload all with `gh release upload --clobber`
6. **Pass signature to downstream job** — use `actions/upload-artifact` to save the signature string for the `publish` job
7. Clean up keychain

### Job 2: `publish` (needs: build)

Runs on `ubuntu-latest` after all three builds complete:

1. Download signature artifacts from all three matrix jobs
2. Extract changelog from `CHANGELOG.md`
3. Generate `latest.json` with three distinct URLs + signatures:
   ```json
   {
     "platforms": {
       "darwin-universal": { "url": ".../Cmdr_universal.app.tar.zst", "signature": "..." },
       "darwin-aarch64":   { "url": ".../Cmdr_aarch64.app.tar.zst",  "signature": "..." },
       "darwin-x86_64":    { "url": ".../Cmdr_x86_64.app.tar.zst",   "signature": "..." }
     }
   }
   ```
4. Update the GitHub release body with changelog notes
5. Commit `latest.json` to main
6. Trigger website deploy webhook

### Artifact naming

| Type | Pattern |
|------|---------|
| DMG | `Cmdr_{version}_aarch64.dmg`, `..._x86_64.dmg`, `..._universal.dmg` |
| Updater | `Cmdr_aarch64.app.tar.zst`, `..._x86_64...`, `..._universal...` |

DMGs are already arch-named by Tauri. Updater artifacts need manual renaming (Tauri names them all `Cmdr.app.tar.zst`).

## Part 2: Website

### 2a. `apps/website/src/lib/release.ts` — export arch-specific URLs

```ts
export const dmgUrls = {
  aarch64: `${base}/Cmdr_${version}_aarch64.dmg`,
  x86_64:  `${base}/Cmdr_${version}_x86_64.dmg`,
  universal: `${base}/Cmdr_${version}_universal.dmg`,
}
// Keep dmgUrl as alias for universal (backward compat)
export const dmgUrl = dmgUrls.universal
```

### 2b. `apps/website/src/layouts/Layout.astro` — inline arch-detection script

Add a small `<script is:inline>` before `</body>` that:

1. Checks `navigator.userAgentData` (Chromium only) — calls `getHighEntropyValues(['architecture'])`
2. Maps `"arm"` → `aarch64`, `"x64"` → `x86_64`
3. Falls back to `universal` if unavailable (Safari, Firefox)
4. Sets `data-mac-arch` on `<html>`
5. Swaps `href` on all `<a data-download-link>` elements using `data-dmg-arm` / `data-dmg-intel` attributes

No WebGL heuristic, no UA sniffing — just `userAgentData` or universal.

### 2c. Download link components — add data attributes

All download `<a>` tags get three attributes for the script to work:
```html
<a href={dmgUrls.universal}
   data-download-link
   data-dmg-arm={dmgUrls.aarch64}
   data-dmg-intel={dmgUrls.x86_64}>
```

**Files to update:**
- `apps/website/src/components/Hero.astro` — CTA button (import `dmgUrls`)
- `apps/website/src/components/Header.astro` — desktop + mobile download buttons
- `apps/website/src/pages/pricing.astro` — download button
- `apps/website/src/components/Download.astro` — primary button + new arch selector (see below)

### 2d. `Download.astro` — arch selector UI

Replace the "Universal (Apple Silicon + Intel)" subtitle with a dynamic label, and add secondary arch links below the main button:

- Subtitle: `data-arch-subtitle` — JS updates to "Apple Silicon" or "Intel" when detected; static default is "Universal (Apple Silicon + Intel)"
- Below the button, add a small row:
  ```
  Also available: Apple Silicon | Intel | Universal
  ```
  Each is a direct download link. JS highlights the detected one.

### 2e. No-JS / Safari / Firefox behavior

All links default to the universal DMG in static HTML. Detection only upgrades the experience for Chromium users (~65-70% of desktop traffic). This is the correct trade-off — no user gets a broken experience.

## Part 3: Updater compatibility

No code changes needed. The Tauri updater reads `latest.json` and selects the platform key matching its compiled target:
- Existing universal installs → `darwin-universal` → universal updater artifact (seamless)
- New arch-specific installs → `darwin-aarch64` / `darwin-x86_64` → smaller updates

## Verification

1. **CI**: Push a test tag (e.g., `v0.5.1-rc.1`) and verify:
   - Three DMGs and three updater archives appear on the GitHub release
   - `latest.json` has three distinct URLs and valid signatures
2. **Website**: `pnpm dev` in `apps/website/`, open in Chrome → verify the download button href swaps to aarch64 on Apple Silicon. Open in Safari → verify it stays on universal.
3. **Updater**: Install from an arch-specific DMG, verify it picks up updates from the matching platform key in `latest.json`
