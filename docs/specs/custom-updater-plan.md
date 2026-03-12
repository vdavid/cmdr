# Custom updater: preserve macOS permissions across updates

## Problem

Tauri's built-in updater replaces the entire `.app` bundle on update (move old to temp, move new into place). This
changes the bundle directory's inode, which causes macOS TCC to lose track of Full Disk Access permissions. Users must
toggle FDA off and on in System Settings after every update.

**Root cause confirmed**: we verified that `rsync`-ing files *into* the existing `.app` (preserving the directory inode
and `com.apple.macl` xattr) keeps TCC grants intact. The code signature remains valid. See the experiment log in the
conversation that produced this spec.

**Tauri GitHub issues**: [#10567](https://github.com/tauri-apps/tauri/issues/10567),
[#11085](https://github.com/tauri-apps/tauri/issues/11085),
[#10779](https://github.com/tauri-apps/tauri/discussions/10779).

## Why not use Apple's standard approach?

Apple's intended update path for non-App Store apps is [Sparkle](https://sparkle-project.org/), which replaces the
entire `.app` bundle — the same approach Tauri uses. The TCC/FDA inode tracking issue is a known macOS bug that Apple
hasn't fixed, and Sparkle has the same problem. The rsync-into-bundle workaround is what the macOS developer community
has converged on. Several apps use this approach, and Apple neither documents nor prohibits it. Code signing remains
valid because all signed content is replaced together, and macOS re-validates at launch time, not continuously. If Apple
ever fixes TCC to use path-based tracking instead of inode-based, this workaround becomes unnecessary but still
harmless.

## Solution

Replace the Tauri updater plugin with a custom updater that:

1. Uses the same `latest.json` manifest and minisign signatures
2. Downloads and verifies the update tarball
3. Installs by syncing files *into* the existing `.app` bundle instead of replacing it

## What stays the same

- `latest.json` format and endpoint (`https://getcmdr.com/latest.json`)
- Minisign signature verification (same key pair)
- Release workflow builds, signs, and uploads the same `.app.tar.gz` artifacts
- `tauri-action` still handles building, code signing, notarization, and uploading to GitHub Releases
- `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets stay
- Frontend UX: silent download → persistent toast → restart button
- Settings: `updates.autoCheck`, `advanced.updateCheckInterval`

## What changes

### Rust backend (new Tauri commands)

New module `src-tauri/src/updater/` with these commands:

| Command | What it does |
|---|---|
| `check_for_update` | Fetches `latest.json`, selects platform entry by arch (see below), compares versions (semver), returns update metadata or null |
| `download_update` | Downloads the `.app.tar.gz` to a temp dir, verifies minisign signature, stores path in shared state |
| `install_update` | Reads path from shared state, extracts tarball, syncs files into the running `.app` bundle (see install strategy below) |

State between commands: a `Mutex<Option<PathBuf>>` in Tauri managed state holds the downloaded tarball path. `download_update` sets it; `install_update` reads and clears it.

### Architecture detection

`check_for_update` builds the platform key `"darwin-{arch}"` using `cfg!(target_arch)` at compile time — `"darwin-aarch64"` on Apple Silicon, `"darwin-x86_64"` on Intel. It looks up that exact key in `latest.json`'s `platforms` map. This matches Tauri's built-in updater behavior.

No `darwin-universal` fallback. All three keys (`darwin-aarch64`, `darwin-x86_64`, `darwin-universal`) exist in
`latest.json`, but the updater always picks the arch-specific one. A user who installed the universal DMG will silently
switch to the smaller arch-specific update — same functionality, smaller download.

### Install strategy (the key part)

```
1. Extract tarball to temp dir → /tmp/cmdr-update-staging/Cmdr.app/
2. Find the running app's bundle path (std::env::current_exe → walk up to .app)
3. Check write permissions to the bundle — if denied, escalate (see § Privilege escalation)
4. Sync files in this order (binary last to minimize signature inconsistency window):
   a. Resources/ (icon, AI dylibs, Assets.car)
   b. Info.plist
   c. _CodeSignature/ and CodeResources
   d. Contents/MacOS/Cmdr (the binary — last)
5. Delete files that exist in the old bundle but not the new one (unconditionally — no whitelist)
6. Run `touch` on the .app bundle (updates modification time, triggers LaunchServices refresh)
7. Clean up temp dir
```

This is conceptually `rsync -a --delete new.app/ old.app/` but implemented in Rust for reliability and error handling.
The `.app` directory itself is never moved or recreated, so its inode and xattrs are preserved.

**Why binary last**: the running process holds the old binary's file descriptor open (standard Unix semantics), so
overwriting on disk is safe. Doing it last means the code signature files (`_CodeSignature`) already match the new
binary content, minimizing the window where the bundle's signature is inconsistent. In practice this window is
irrelevant — macOS validates signatures at launch, not continuously — but it's good hygiene.

**Why unconditional deletion**: users cannot meaningfully modify files inside an app bundle (Finder doesn't browse into
them, and any modification breaks code signing). macOS metadata like `.DS_Store` or Spotlight indexes are stored
externally, not inside bundles. The `_CodeSignature/CodeResources` manifest lists every file that must be present, so
stray files would break validation anyway.

**Bundle contents** (verified from current production build — only 32 files, ~63 MB total):

| Path | Size | Count |
|---|---|---|
| `Contents/MacOS/Cmdr` | 33 MB | 1 binary |
| `Contents/Resources/resources/ai/` | 25 MB | ~20 dylibs + llama-server |
| `Contents/Resources/icon.icns` | tiny | 1 |
| `Contents/Resources/Assets.car` | tiny | 1 |
| `Contents/Info.plist` | tiny | 1 |
| `Contents/_CodeSignature/`, `CodeResources` | tiny | 2 |

No `Frameworks/` directory (Tauri uses the system WebView). Frontend assets are compiled into the binary. At 63 MB on a
local SSD, the sync completes in well under a second.

**Stale/broken downloads**: `download_update` always writes to the same temp path and overwrites any existing file.
A previous incomplete download is simply replaced — no cleanup logic needed for <63 MB files.

**Crash during sync**: accepted risk. The bundle is only 32 files and the sync is sub-second on local disk, making a
crash during this window extremely unlikely. If it does happen, the user can re-download the `.dmg` from the website and
drag the app back to `/Applications`. No backup/rollback mechanism is needed at this stage.

**Tested**: we confirmed that replacing all files inside a running Cmdr.app bundle (including the binary) while the app
was running preserved TCC/FDA permissions. After quitting and relaunching, the app worked correctly with permissions
intact and the code signature valid.

### Privilege escalation

When the app can't write to its own bundle (e.g., installed by an admin/MDM on a multi-user Mac, or owned by root), the
installer needs privilege escalation.

**Flow**:

1. Try writing directly (works for user-installed apps — the common case)
2. If permission denied → escalate via `osascript -e 'do shell script "..." with administrator privileges'`
3. macOS shows the standard system "Cmdr wants to make changes" password dialog (username + password)
4. The shell script performs the file sync as root

This is the same mechanism Sparkle (the dominant macOS update framework) uses. It's been stable since macOS 10.0, works
on all macOS versions, and requires no additional binaries, XPC services, or code signing setup — just a single
`std::process::Command` call in Rust. The tradeoff: the password prompt appears on every update that needs escalation
(as opposed to a persistent helper that remembers).

**Future upgrade path**: for enterprise/MDM scenarios where IT needs to pre-authorize or silently push updates, upgrade
to `SMAppService` (macOS 13+). This installs a persistent privileged helper daemon via XPC — the password prompt only
appears once, and MDM can pre-install the helper silently. This requires a signed helper binary, a `launchd` plist, and
additional code signing setup. Defer until enterprise customers need it.

### Signature verification

Tauri's updater uses [minisign](https://jedisct1.github.io/minisign/) (Ed25519-based) via the `minisign-verify` crate.
Use it directly — it's already in `Cargo.lock` as a transitive dep of `tauri-plugin-updater`, so adding it as a direct
dep costs zero extra compilation.

Both the public key (from `tauri.conf.json`) and signature (from `latest.json`) are **double-encoded**:
base64(minisign-text-format). Decode the outer base64 first, then parse with `PublicKey::decode()` /
`Signature::decode()`. This matches exactly what Tauri does internally (verified from `tauri-plugin-updater` source).

```rust
use minisign_verify::{PublicKey, Signature};

let pubkey_text = String::from_utf8(BASE64.decode(pubkey_base64)?)?;
let sig_text = String::from_utf8(BASE64.decode(sig_base64)?)?;
let public_key = PublicKey::decode(&pubkey_text)?;
let signature = Signature::decode(&sig_text)?;
public_key.verify(data, &signature, true)?; // allow_legacy=true, same as Tauri
```

Read the public key from `tauri.conf.json` at compile time or hardcode it as a constant — either works since the key
doesn't change between releases.

Note: `tauri-utils` has no public signature verification API — the logic is private inside `tauri-plugin-updater`.

### Frontend changes

`updater.svelte.ts` switches from `@tauri-apps/plugin-updater` to invoking the new Tauri commands on **all platforms**:

```typescript
// Before:
import { check } from '@tauri-apps/plugin-updater'
const update = await check()
await update.downloadAndInstall()

// After:
import { invoke } from '@tauri-apps/api/core'
const update = await invoke('check_for_update')
await invoke('download_update', { url: update.url, signature: update.signature })
await invoke('install_update')
```

No platform branching in the frontend. The Rust backend handles platform differences: on macOS, the custom updater does
the rsync-into-bundle install; on Linux/Windows, the same commands delegate to the Tauri updater plugin internally. This
keeps the frontend simple and makes the backend the single source of truth.

State machine and toast logic stay the same. `relaunch()` from `@tauri-apps/plugin-process` still handles the restart.

### Disable Tauri updater plugin registration on macOS

The Tauri updater plugin stays in Cargo.toml and package.json (needed for Linux/Windows and for the bundler). Only its
`.plugin()` registration in `lib.rs` is gated.

| File | Change |
|---|---|
| `src-tauri/src/lib.rs` | Gate plugin registration with `#[cfg(not(target_os = "macos"))]` instead of removing it |
| `src-tauri/Cargo.toml` | Keep `tauri-plugin-updater` (still needed for Linux/Windows) |
| `apps/desktop/package.json` | Keep `@tauri-apps/plugin-updater` (still needed for Linux/Windows) |
| `src-tauri/tauri.conf.json` | Keep `plugins.updater` — the bundler requires it when `createUpdaterArtifacts` is true (verified: removing it causes `plugins > updater doesn't exist` error) |
| `src-tauri/tauri.conf.json` | Keep `bundle.createUpdaterArtifacts: true` |
| `apps/desktop/src/lib/updates/updater.svelte.ts` | Replace plugin API with `invoke()` calls (all platforms) |

### CI guard

The current runtime guard (`std::env::var("CI")`) that skips the updater plugin stays but moves into the custom updater:
`check_for_update` returns `None` when `CI` is set, so no network call happens in CI. The custom updater commands
themselves are registered unconditionally — they're inert until called.

### Release workflow

No changes needed. `tauri-action` generates the updater artifacts (`.app.tar.gz` + `.sig`) based on
`createUpdaterArtifacts` in `tauri.conf.json`, not based on the updater plugin being present. The publish job's
`latest.json` generation stays the same.

**Verify this assumption before starting implementation.**

## Files to create/modify

### New files
- `src-tauri/src/updater/mod.rs` — commands: `check_for_update`, `download_update`, `install_update`
- `src-tauri/src/updater/manifest.rs` — `latest.json` parsing and version comparison
- `src-tauri/src/updater/signature.rs` — minisign verification
- `src-tauri/src/updater/installer.rs` — the rsync-into-bundle logic

### Modified files
- `src-tauri/src/lib.rs` — gate updater plugin to non-macOS, register new custom updater commands
- `src-tauri/Cargo.toml` — add `minisign-verify`, `flate2`, `tar`
- `apps/desktop/src/lib/updates/updater.svelte.ts` — replace plugin API with `invoke()` calls (all platforms)
- `apps/desktop/src/lib/updates/CLAUDE.md` — update architecture docs

### No changes needed
- `UpdateToastContent.svelte` — still calls `relaunch()`, no change
- `.github/workflows/release.yml` — still uses tauri-action, still generates same artifacts
- `apps/website/public/latest.json` — same format

## New Rust dependencies

| Crate | Purpose | Notes |
|---|---|---|
| `minisign-verify` | Signature verification | Lightweight, ~200 lines, no unsafe. Same algo Tauri uses internally |
| `flate2` | gzip decompression | Likely already an indirect dep |
| `tar` | Tarball extraction | Likely already an indirect dep |

`reqwest` is already a direct dependency (v0.12, with `json`, `rustls-tls`, `stream` features,
`default-features = false`). No need to add it.

Check latest versions from crates.io and license compatibility (`cargo deny check`) before adding.

## Accepted gaps

- **Dev mode**: the custom updater commands work in dev mode with no guard. Could theoretically update
  `/Applications/Cmdr.app` while running `pnpm dev`. Low risk — add a dev-mode check later if it becomes bothersome.

## Implementation order

1. ~~**Verify** that `tauri-action` still generates `.app.tar.gz` + `.sig` without the updater plugin~~ **Verified** —
   artifacts are generated based on `createUpdaterArtifacts` config, not the plugin. The `plugins.updater` JSON config
   must stay (bundler reads the pubkey from it), but the Rust crate registration can be skipped on macOS.
2. **Build the Rust backend**: manifest parsing → signature verification → download → install
3. **Wire up the frontend**: swap `@tauri-apps/plugin-updater` calls for `invoke()` calls
4. **Gate the plugin to non-macOS**: `#[cfg]` in lib.rs, keep Cargo.toml/tauri.conf.json/package.json (still needed for Linux/Windows)
5. **Test end-to-end**: build a release, serve a local `latest.json`, verify update + FDA preservation

## Resolved questions

- **`createUpdaterArtifacts` independence**: confirmed — artifacts are generated based on the config flag, not the
  Rust plugin. The `plugins.updater` JSON block must stay (bundler reads the pubkey), but the Rust crate can be gated
  to non-macOS only. The plugin stays for Linux/Windows where the TCC issue doesn't exist.
- **Rollback strategy**: no backup needed. The sync is sub-second on 32 files. If it fails, user re-downloads the DMG.
- **File deletion policy**: delete all old-but-not-in-new files unconditionally. No whitelist needed — users don't
  modify bundle contents, and stray files break code signing anyway.
- **Running binary replacement**: tested and confirmed safe. Unix file descriptor semantics keep the old process running.
  macOS validates code signatures at launch, not continuously.
- **reqwest feature conflict**: not an issue — `reqwest` is already a direct dep with `rustls-tls`.

## Resolved open questions

- **Should we keep `createUpdaterArtifacts` or generate tarballs ourselves?** Keep it. The format is a standard
  `.tar.gz` — no risk of breaking changes, and it avoids duplicating tauri-action's work.
- **Do we need download progress reporting?** No. The download is <63 MB and happens silently in the background. No
  progress bar needed. Can add Tauri event streaming later if desired.

