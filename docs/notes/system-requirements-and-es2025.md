# System requirements and ES2025 adoption notes

Reference for when we revisit minimum-OS pinning and adopting newer JS features. Assembled 2026-04-27 during the npm /
Rust / Go dep sweep that followed the v0.14.0 release.

## Current effective minimums (nothing declared)

We don't pin a `minimumSystemVersion` in `tauri.conf.json` and the website doesn't list a minimum OS version either;
only "macOS (Apple Silicon and Intel)" and "Linux: alpha." So the floors below are implicit, imposed by the stack.

### macOS

- **Tauri 2 runtime (WKWebView, FFI bindings)**: macOS 10.15 Catalina (2019-10)
- **Apple Silicon binary (arm64)**: macOS 11.0 Big Sur (2020-11), M1 ships with this
- **Intel binary (x86_64)**: macOS 10.15 Catalina (2019-10)
- **Universal binary (what we ship)**: Per-arch: 10.15 Intel, 11.0 Apple Silicon
- **Apple frameworks we touch (`IOKit`, `core-foundation`, `FSEvents`, etc)**: Ancient, not a binding constraint
- **Modern CSS we use (`:has()`, container queries, top-level await)**: macOS 12 Monterey (2021-10) for things to render
  correctly
- **llama-server (AI feature only)**: Apple Silicon only (no Intel AI build, rest of app works fine)

**Effective practical floor: macOS 12 Monterey (2021-10).** Anyone older may launch the app but see CSS render oddities.

### Linux

- **Tauri 2 runtime needs WebKitGTK 4.1**: Ubuntu 22.04 (2022-04), Fedora 36+ (2022-05), Debian 12 (2023-06). 4.0
  doesn't work.
- **Our build target's `libwebkit2gtk-4.1-dev`**: Same
- **`glibc 2.31+`**: Ubuntu 20.04+, Debian 11+, Fedora 32+
- **Linux SMB / MTP / inotify / FUSE / libudev**: Anything from the last decade
- **Secret Service via `zbus-secret-service-keyring-store`**: GNOME 3.x with `gnome-keyring-daemon`, or KDE Plasma 5.x
  with `kwalletmanager`. Headless servers fall back to our cocoon-encrypted file.
- **Trash via `trash` crate (FreeDesktop spec)**: Any modern DE

**Effective practical floor: Ubuntu 22.04 LTS / Fedora 36 / Debian 12 (2022-04 and later).** WebKitGTK 4.1 is the
tightest constraint.

## ES2025 features and where they're available

Sourced from WebKit feature status and MDN baseline data. Cmdr doesn't currently use any of these; this table is for
future reference when we consider adopting them.

| Feature                                       | WebKit | macOS Safari/WKWebView floor  | Linux WebKitGTK | Windows WebView2 (Chromium) |
| --------------------------------------------- | ------ | ----------------------------- | --------------- | --------------------------- |
| `Set.union/intersection/difference` etc.      | 17.4   | macOS 13.6.5 / 14.4 (2024-03) | 2.44+ (2024-04) | Chrome 122 (2024-02)        |
| Iterator helpers (`.map().filter()`)          | 18.2   | macOS 14.7.2 / 15.2 (2024-12) | 2.46+           | Chrome 122 (2024-02)        |
| `using` / `await using` (resource management) | 18.0   | macOS 14.7 / 15.0 (2024-09)   | 2.46+           | Chrome 134 (2025-03)        |
| `Promise.try`                                 | 18.2   | macOS 14.7.2 / 15.2 (2024-12) | 2.46+           | Chrome 128 (2024-08)        |
| `RegExp.escape`                               | 18.4   | macOS 14.7.5 / 15.4 (2025-03) | 2.48+           | Chrome 136 (2025-04)        |
| `Float16Array` (we don't need)                | 17.4   | 13.6.5 / 14.4                 | 2.44+           | Chrome 122                  |

### What each ES2025 feature would let us simplify

- **`Set.union/intersection`**: `FilePane.svelte` `SvelteSet<number>` selection adjustments: replaces hand-rolled
  union/intersect with native methods.
- **`using` / `await using`**: Manual `try/finally` for cleanup: closing streams, releasing locks, unlistening Tauri
  events. `auto-send-toast.svelte.ts`, `network-store.svelte.ts`, file-explorer disposal flows. The biggest QoL win.
- **`Promise.try`**: Wraps a sync-or-async function in a Promise and catches sync throws, cleaner than
  `new Promise((resolve) => resolve(maybeThrow()))`.
- **Iterator helpers**: Wherever we do `Array.from(iter).map().filter()`: drops the intermediate array allocation.
- **`RegExp.escape`**: Search pattern building: replaces our hand-rolled `\\$&` escape with a one-liner.
- **`Float16Array`**: Not relevant.

### What's safe to adopt today

Without bumping our floor: only `Set.union` and friends. They need only macOS 13.6.5+, which most current Macs have.
Everything else needs macOS 14.7+, so adopting them means declaring that floor and updating the website.

## Recommendation if we want to use the fancier ES2025 features

1. Add `"minimumSystemVersion": "14.7"` (or "15.0") to `tauri.conf.json` macOS section.
2. Update website's download page system requirements to mention the version.
3. Bump tsconfigs to `target: ES2025` so TypeScript knows these exist without manual `lib` overrides.
4. Adopt `using` / `Set.union` / iterator helpers selectively where they shorten code.
5. For Linux: ask the alpha tester what distro they're on. Anything beyond `Set` methods needs WebKitGTK 2.46+, which
   Ubuntu 24.04 doesn't ship (24.04 has 2.44).

If we don't want to raise the floor, just keep `target: esnext` (which we already do) and skip these features.

## Other simplifications worth remembering for next time

These came up during the dep sweep and we deferred them or skipped them:

- **TypeScript 6.0 default flips.** TS 6 defaults `esModuleInterop: true`, `moduleResolution: bundler`,
  `target: ES2025`, `types: []`. Our tsconfigs explicitly set most of these. Could remove the redundant settings, but
  explicit is more readable than implicit. Skip unless aiming for minimalism.
- **mtp-rs `download_partial_64()`.** Available since 0.13. Useful if we ever add resume-on-MTP-download for >4 GB
  files. Not actionable today.
- **vite-plugin-svelte 7 inspector integration.** Would matter if we'd had a separate `vite-plugin-svelte-inspector`
  dep. We don't.
- **satori 0.26 builtin JSX runtime.** Would mean rewriting `og/[slug].png.ts` for marginal benefit.
- **zip 8.6 better encryption / ZIP64.** We use basic deflate for log bundles; nothing to gain.
