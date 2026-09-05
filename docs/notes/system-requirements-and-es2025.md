# System requirements and ES2025 adoption notes

Reference for minimum-OS pinning and for adopting newer JS features.

## The declared floor and what backs it

Three numbers, and nothing but this note ties them together:

- **`bundle.macOS.minimumSystemVersion` in `apps/desktop/src-tauri/tauri.conf.json`** is what the `.app` claims: macOS
  10.15 Catalina. Two checks hold the native side to it. `desktop-rust-macos-availability` enforces every Objective-C
  selector against it, so anything newer needs a `crate::platform::macos_at_least` gate plus the
  `allowed-newer-selector` marker. `desktop-macos-framework-floor` enforces every framework the built binary LOADS
  against it, which is the harder half: a framework link is a hard `LC_LOAD_DYLIB` that dyld resolves before `main`, so
  a too-new one is not a call that might not happen, it's an app that won't open and can't be gated out of it.
- **`build.target` in `apps/desktop/vite.config.js`** is what the frontend bundle is transpiled to: `safari15`.
  `build.cssTarget` follows it, so JS and CSS share one floor. `desktop-vite-build-target` fails the build if the pin
  goes missing or stops naming a Safari version, but it deliberately enforces no relationship between the numbers:
  mapping a macOS version to "the WebKit we must assume" is a product call, not a fact.
- **The capability floor the app enforces at runtime**, Safari 15.4, in `apps/desktop/src/lib/utils/webkit-compat.ts`
  and the inline guard in `apps/desktop/src/app.html`. This one isn't a knob: it's what the shipped code actually needs,
  and the guard is what turns "below it" from a white screen into a sentence the user can act on.

Catalina is a **best-effort** floor, deliberately below what the app needs on a stock install. The three numbers don't
line up, and that's the design:

- `minimumSystemVersion` is the OLDEST macOS whose Objective-C surface we stay inside, so Gatekeeper lets the app open
  and the native code doesn't abort. It says nothing about whether the UI can render.
- A stock Catalina 10.15.0 ships Safari 13, a stock Big Sur 11.0 ships Safari 14, and a stock Monterey 12.0 ships Safari
  15.0. The frontend needs newer than all three.
- A **fully patched** Catalina 10.15.7 reaches Safari 15.6.1, Big Sur 11.7.10 reaches 16.6, and Monterey 12.7.6 reaches
  17.6, and every one of those runs the frontend fine. That's the whole reason Catalina is worth offering: "install your
  updates" is advice a user can act on (verified against WebKit's release history and Apple's security-update index,
  2026-09-02).

Below the capability floor the app blocks itself rather than white-screening. Why the block can't live inside the app: §
The two floors old WebKit crosses.

Leaving `build.target` unset is what the check exists to prevent. Vite's default is
`ESBUILD_BASELINE_WIDELY_AVAILABLE_TARGET`, a MOVING baseline (Safari 16.4 under Vite 8), so an unset target drifts
upward on a routine dep bump while the plist stays put. That gap was live: the bundle carried 286 raw `oklch()` colors
(Safari 15.4+), so every one of those design tokens was unset on a Monterey that never updated Safari. Pinning
`safari15` lowers them to `lab()` (Safari 15.0) and costs +8,939 bytes on a 6.2 MB bundle (verified on Vite 8.2.0,
production build, 2026-09-02).

The website says the same two-part answer: `apps/website/src/components/Download.astro` (the download card's fine
print), `llms.txt.ts` / `llms-full.txt.ts`, and `brand/listings/macupdate.md`. Keep them in step with the plist.

## The two floors old WebKit crosses

Old WebKit fails Cmdr in two different places, and only one of them is something a bundle target could fix.

**Floor 1, parsing (Safari 14).** Measured by parsing every emitted client chunk of a production build with `acorn` at
successive `ecmaVersion` settings (93 chunks, Vite 8.2.0 production build, 2026-09-02):

- `ecmaVersion: 2019`: 65 of 93 chunks fail (optional chaining, `??`).
- `ecmaVersion: 2020`: 24 of 93 fail (logical assignment `||=` / `&&=` / `??=`).
- `ecmaVersion: 2021` and up: all 93 parse.

Logical assignment shipped in Safari 14.0, optional chaining in 13.1, so Safari 13.x cannot PARSE the bundle at all,
whatever its runtime APIs say. The shell's SvelteKit boot script is a classic script calling `import(...)`; that import
rejects with a syntax error nothing handles, the loading spinner never goes away, and the user sees a blank window.
Lowering `build.target` wouldn't rescue it, because floor 2 stands regardless.

**Floor 2, runtime APIs (Safari 15.4).** esbuild's `target` lowers SYNTAX and never runtime APIs, so a `safari15` bundle
still calls whatever the source calls. `crypto.randomUUID` (15 files), `Object.hasOwn`, `Array.prototype.findLast`, and
the `:has()` selector all arrived in Safari 15.4. Nothing lowers those, so 15.4 is the hard floor below which the app
genuinely cannot work.

Together they're why the block screen is an inline ES5 `<script>` in `apps/desktop/src/app.html` that runs before the
module boot script: a screen shipped inside the bundle is unreachable on exactly the Safari (13.x) that most needs it.
Its translated copy is spliced in at build time from the message catalogs, so the shell can't drift from what
translators wrote; the mechanism is in `apps/desktop/src/lib/utils/DETAILS.md` § Old-WebKit boot guard.
`webkit-compat.ts` runs the same capability probe inside the app, for the code that can reach it.

## Effective minimums imposed by the stack

### macOS

- **Tauri 2 runtime (WKWebView, FFI bindings)**: macOS 10.15 Catalina (2019-10)
- **Apple Silicon binary (arm64)**: macOS 11.0 Big Sur (2020-11), M1 ships with this
- **Intel binary (x86_64)**: macOS 10.15 Catalina (2019-10)
- **Universal binary (what we ship)**: Per-arch: 10.15 Intel, 11.0 Apple Silicon
- **Apple frameworks the binary loads**: all 28 of them predate Catalina, the newest by two years (`ColorSync`,
  `CoreML`, and `Vision`, all 10.13). That is a fact about today's build rather than a property of the stack, which is
  the correction v0.42.0 bought: a dependency can put a framework in the binary for code nothing calls, and
  `UniformTypeIdentifiers` (macOS 11) rode in behind an `objc2-quick-look-ui` feature default and made the app
  unlaunchable on Catalina. `desktop-macos-framework-floor` reads the load commands now, so this line stays true by
  enforcement rather than by assumption (verified by `otool -L` on the `aarch64-apple-darwin` build, 2026-09-05).
- **Modern CSS**: nothing above the `build.target` floor ships. Container queries (Safari 16) are gone from the tree and
  banned by stylelint (`at-rule-disallowed-list` / `property-disallowed-list` in `apps/desktop/.stylelintrc.mjs`); the
  stand-in is `useInlineSize` in `apps/desktop/src/lib/utils/inline-size-action.ts`. `:has()` (Safari 15.4) sits ON the
  capability floor, so the boot guard covers it. The one thing esbuild can't lower is `color-mix()` over a `var()`,
  which is why the `@supports not (…)` fallbacks in `app.css` and the runtime mixes behind `webkit-compat.ts` both have
  to stay.
- **llama-server (AI feature only)**: Apple Silicon only (no Intel AI build, rest of app works fine)

**Effective practical floor: macOS 10.15 Catalina (2019-10)**, the version the plist declares, on the best-effort terms
above. macOS 12 Monterey and up is the version range Cmdr is actually developed and tested against.

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

1. Raise `minimumSystemVersion` in `tauri.conf.json` to `14.7` (or `15.0`), `build.target` in `vite.config.js` to the
   matching Safari, and the capability floor in `webkit-compat.ts` plus the `app.html` guard with them.
2. Update website's download page system requirements to mention the version.
3. Bump tsconfigs to `target: ES2025` so TypeScript knows these exist without manual `lib` overrides.
4. Adopt `using` / `Set.union` / iterator helpers selectively where they shorten code.
5. For Linux: ask the alpha tester what distro they're on. Anything beyond `Set` methods needs WebKitGTK 2.46+, which
   Ubuntu 24.04 doesn't ship (24.04 has 2.44).

If we'd rather keep the floor where it is, leave `build.target` on `safari15` and skip these features. Note that raising
the floor takes all three numbers: the plist, `build.target`, and the capability floor.

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
