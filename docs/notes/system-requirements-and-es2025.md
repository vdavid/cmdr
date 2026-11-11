# System requirements and ES2025 adoption notes

Reference for minimum-OS pinning and for adopting newer JS features.

## The declared floor and what backs it

Three numbers, and nothing but this note ties them together:

- **`bundle.macOS.minimumSystemVersion` in `apps/desktop/src-tauri/tauri.macos.conf.json`** is what the `.app` claims:
  macOS 10.15 Catalina. Three checks hold the native side to it, one per way the promise can break.
  `desktop-rust-macos-availability` enforces every Objective-C selector against it, so anything newer needs a
  `crate::platform::macos_at_least` gate plus the `allowed-newer-selector` marker. `desktop-macos-framework-floor`
  enforces every framework the built binary LOADS, which is the harder half: a framework link is a hard `LC_LOAD_DYLIB`
  that dyld resolves before `main`, so a too-new one is not a call that might not happen, it's an app that won't open
  and can't be gated out of it. `desktop-macos-symbol-floor` enforces the symbols INSIDE those frameworks, the gap
  between the two: `kIOMainPortDefault` (macOS 12) lives in an IOKit that exists all the way back, and v0.46.0 aborted
  at launch on 10.15 and 11 with every framework present and every selector fine.
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
  17.6 (verified against WebKit's release history and Apple's security-update index, 2026-09-02). Safari's version is
  only the WebKit an APP gets because Cmdr opts into Safari's staged copy; without that, a fully patched Catalina still
  hands Cmdr Safari 13.1's WebKit (§ The WebKit an app gets on older macOS). With it, every one of those runs the
  frontend, and that's the whole reason Catalina is worth offering: "install your updates" is advice a user can act on.

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

The same gap runs the other way: a function NEWER than 15.4 (`toSorted`, `Object.groupBy`, `Set.union`) ships as written
and throws `TypeError` on Catalina, breaking just the feature that calls it, so a startup smoke test can miss it. Three
guards hold the line, all reading what's "newer" from MDN's browser-compat-data (`@mdn/browser-compat-data`, through
`apps/desktop/scripts/vite-webkit-floor-compat.ts`, which also holds the floor constant), so no hand-kept list goes
stale:

- **JavaScript built-ins in the app's code**: `apps/desktop/tsconfig.json` pins `lib` to ES2022, and
  `apps/desktop/src/webkit-floor.d.ts` adds back the ES2023 members Safari 15.4 has (`findLast`, `findLastIndex`), so a
  newer one fails type-checking.
- **DOM calls in the app's code**: the type-aware lint rule `cmdr/webkit-floor-dom`
  (`apps/desktop/eslint-plugins/webkit-floor-dom.js`) asks TypeScript which declaration a call resolves to and looks up
  exactly that BCD entry (`api.Element.checkVisibility`, `api.AbortSignal.timeout_static`). TypeScript's DOM lib can't
  be versioned, and matching DOM method names in the bundle is hopeless: generic ones (`info`, `format`, `destroy`)
  collide with libraries' own methods.
- **Everything shipped, dependencies included**: `vite-webkit-api-floor.ts` scans each emitted client chunk for calls to
  newer globals, owner-qualified statics, and JavaScript prototype methods whose name no older API shares. Hand-checked
  uses sit in its `REVIEWED` list with the reason (devalue's feature-checked base64, LogTape's own `with` and `drop`).

A call inside a check that names the function passes all three. On adding them, the bundle and the app's source had six
hits, all already guarded through indirection the checks can't follow (a helper, a worker started behind a main-thread
check), now marked with reasons (Vite 8.3.5 production build, BCD 8.1.8, 2026-11-05).

**Beside the floors, ICU.** A staged Safari brings its own JavaScriptCore but keeps the OS's ICU, and some `Intl` APIs
exist only on a newer ICU than Catalina's. So Safari 15.6.1 on Catalina still has no `Intl.ListFormat`, which crashed
0.55.0's main window there (cmdr-reports#80). The app degrades rather than blocks on this; the ICU gates, the affected
APIs, and the guardrail are in `apps/desktop/src/lib/intl/DETAILS.md` § Catalina has no Intl.ListFormat.

**Beside the floors, regex syntax.** A regex feature has no lowering either, and the bundler hides that: given a literal
the target can't parse, it emits `RegExp(`…`, `u`)` instead, silently, which turns a parse error into a runtime
`SyntaxError` wherever the regex is built. 0.59.0 shipped one module-level lookbehind (Safari 16.4) in
`apps/desktop/src/lib/utils/display-path.ts`, so on Catalina's Safari 15.6.1 the module graph threw
`Invalid regular expression: invalid group specifier name` while evaluating, before the logger existed: no frontend log
line at all, and the backend's 10 s fallback showed a blank window (verified on the Catalina test Mac, 10.15.8 and
Safari 15.6.1, by loading the build in Safari with an error-reporting hook, 2026-11-04).
`apps/desktop/scripts/vite-webkit-regex-floor.ts` (run by the `webkitFloor` plugin) now fails `vite build` on any
lookbehind, `v` flag, inline modifier, or duplicate group name in the emitted client chunks, dependencies included.

Together they're why the block screen is an inline ES5 `<script>` in `apps/desktop/src/app.html` that runs before the
module boot script: a screen shipped inside the bundle is unreachable on exactly the Safari (13.x) that most needs it.
Its translated copy is spliced in at build time from the message catalogs, so the shell can't drift from what
translators wrote; the mechanism is in `apps/desktop/src/lib/utils/DETAILS.md` § Old-WebKit boot guard.
`webkit-compat.ts` runs the same capability probe inside the app, for the code that can reach it.

## The WebKit an app gets on older macOS

On a macOS older than the Safari it runs, a Safari update doesn't replace the system WebKit. It installs its own WebKit,
WebCore, and JavaScriptCore under `/Library/Apple/System/Library/StagedFrameworks/Safari/` and leaves
`/System/Library/Frameworks/WebKit.framework` at the version the OS shipped with. Safari gets the staged copy because
its executable asks for it; every other app gets the OS's, unless it asks the same way.

- **Measured on a fully patched Catalina** (a beta user's Mac reporting macOS 10.15.8 (19H2036) and Safari 15.6.1,
  `defaults read <framework>/Resources/Info.plist CFBundleVersion`, 2026-09-12): the system `WebKit.framework` is
  `15609.4.1.1.1`, Safari 13.1's generation, and the staged one is `15613.3.9.1.16`, Safari 15.6.1's. Cmdr ran on the
  system one there, so the boot guard blocked it, and its advice to install updates couldn't help.
- **The opt-in is a load command in the main executable**: `LC_DYLD_ENVIRONMENT` carrying
  `DYLD_VERSIONED_FRAMEWORK_PATH=<staged dir>`, which `ld -dyld_env` writes. WebKit's own build does exactly this when
  it's built for staging (`OTHER_LDFLAGS_VERSIONED_FRAMEWORK_PATH_YES` in
  `Source/WebKit/Configurations/BaseTarget.xcconfig` on WebKit's `safari-613-branch`, with the `/Library/Apple` prefix
  from 10.15 on). Cmdr links the same entry from `apps/desktop/src-tauri/build.rs`, and `desktop-macos-framework-floor`
  fails a binary that lacks it in any slice.
- **It survives hardened runtime** (read from Catalina's dyld, `dyld-750.6` `src/dyld2.cpp`, 2026-09-12). For a
  restricted process dyld strips `DYLD_*` variables from the environment, but `pruneEnvironmentVariables` first applies
  the main executable's own `LC_DYLD_ENVIRONMENT` `DYLD_*_PATH` entries (`checkLoadCommandEnvironmentVariables`). So
  there's no need for `com.apple.security.cs.allow-dyld-environment-variables`, which would reopen library injection. A
  versioned path also makes dyld skip its dyld3 launch closure and resolve through `checkVersionedPaths`, which loads
  whichever copy has the higher `current_version`.
- **What it changes elsewhere**: a Mac whose staged copy isn't newer than the OS's, or that has no staged directory at
  all (macOS 26, verified 2026-09-12), loads the system WebKit exactly as before. A Mac that took a Safari update newer
  than its OS gets the WebKit Safari itself runs on that machine. The entry costs about 2–3 ms of launch time even with
  nothing staged: a minimal hardened, WebKit-linked probe took 8.0–13.7 ms per launch without it and 9.9–16.4 ms with it
  (300 launches per variant, three alternating rounds, macOS 26.6.2 on Apple Silicon, 2026-09-13). Next to Cmdr's own
  startup that's noise, which is why it's unconditional.
- **It works on Intel Catalina** (0.59.0 on the Catalina test Mac, 10.15.8, `lsof -p <Cmdr pid>` and `ps` on its WebKit
  XPC services, 2026-11-04): Cmdr maps the staged `WebKit.framework` and `JavaScriptCore.framework`, and its WebContent,
  Networking, and GPU processes run from the staged copy.
- **Open question** (2026-09-12): on Apple Silicon Big Sur, WebKit's config links `DYLD_FRAMEWORK_PATH` instead for
  arm64 on the macOS 11 SDK, so the versioned opt-in may not reach an M1 whose OS WebKit is below the floor.

### Testing the frontend on the Catalina Mac

CI has no Catalina runner, so the Catalina test Mac (`ssh air`; details in the `air2015` note in David's vault) is the
check. Two ways, cheapest first:

- **Safari, no app build** (about a minute): serve `apps/desktop/build/` from the laptop with a script injected at the
  top of `<head>` that POSTs `error`, `unhandledrejection`, and `console.error` back to the server, reverse-tunnel the
  port with `ssh -R <port>:127.0.0.1:<port> air`, and `open -a Safari http://127.0.0.1:<port>/` there. Safari runs the
  same staged WebKit and ICU Cmdr gets, so a parse or module-evaluation error shows up with its chunk and column. Past
  that point, the only errors are the missing `window.__TAURI_INTERNALS__`.
- **The real app** (about five minutes warm, eight from a fresh worktree):
  `node scripts/tauri-wrapper.ts build --no-bundle --target x86_64-apple-darwin -- --profile e2e` in `apps/desktop/`,
  `scp` an installed `Cmdr.app` from the Air to the laptop, swap in `target/x86_64-apple-darwin/e2e/Cmdr`,
  `codesign --force --deep -s -` it HERE (the Air has no command-line tools, so `codesign` fails there for want of
  `codesign_allocate`), copy it back under `~`, and `open` it over `ssh`. `FE:` lines in
  `~/Library/Logs/com.veszelovszki.cmdr/cmdr.log` mean the frontend mounted. Anything about painting needs the lid open:
  closed, the window is on no screen and WebKit draws no frame at all (2026-11-05). The jsc in Catalina's own
  `JavaScriptCore.framework` is Safari 13's, and it can't load the staged framework (missing symbols), so it's no help.

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
- **Modern CSS**: nothing newer than the floor ships without a fallback or a reviewed reason.
  `apps/desktop/scripts/vite-webkit-css-floor.ts` checks every emitted stylesheet against BCD (properties, keyword
  values, functions, selectors, at-rules, media features) and fails the build on anything Safari 15.4 drops that the
  cascade doesn't cover; its `REVIEWED_CSS` list holds the accepted ones, `harmless` (scrollbar styling, `text-wrap`) or
  `degrades` (none today). A fallback goes in an `@supports not (…)` block: Lightning CSS drops the earlier of two plain
  declarations whenever its own data thinks the target has the newer value, so `overflow: hidden; overflow: clip` ships
  as `clip` alone (lightningcss 1.33.0, 2026-11-05). The `subgrid` rows (servers hub, acknowledgements),
  `contain: inline-size`, and `overflow: clip` fallbacks were checked in Safari 15.6.1 on the Catalina test Mac against
  the build's real CSS, with a static page (2026-11-05). Container queries (Safari 16) are also banned by stylelint
  (`at-rule-disallowed-list` / `property-disallowed-list` in `apps/desktop/.stylelintrc.mjs`); the stand-in is
  `useInlineSize` in `apps/desktop/src/lib/utils/inline-size-action.ts`. `:has()` (Safari 15.4) sits ON the capability
  floor, so the boot guard covers it. `color-mix()` over a `var()` can't be lowered, which is why the
  `@supports not (…)` fallbacks in `app.css` and the runtime mixes behind `webkit-compat.ts` both have to stay (the CSS
  check reads those fallbacks).
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

None of them, while the floor is Catalina's Safari 15.4. Even `Set.union` and friends need Safari 17, which a Catalina
Mac never gets, and the ES2022 `lib` pin plus the bundle scan (§ The two floors old WebKit crosses) fail the check on
any of them. Adopting them means raising the floor (below).

## Recommendation if we want to use the fancier ES2025 features

1. Raise `minimumSystemVersion` in `tauri.macos.conf.json` to `14.7` (or `15.0`), `build.target` in `vite.config.js` to
   the matching Safari, and the capability floor in `webkit-compat.ts` plus the `app.html` guard with them.
2. Update website's download page system requirements to mention the version.
3. Raise the `lib` pin in `apps/desktop/tsconfig.json` to match (and trim `src/webkit-floor.d.ts`), and drop the entries
   the new Safari has from `API_LIST` in `apps/desktop/scripts/vite-webkit-api-floor.ts`.
4. Adopt `using` / `Set.union` / iterator helpers selectively where they shorten code.
5. For Linux: ask the alpha tester what distro they're on. Anything beyond `Set` methods needs WebKitGTK 2.46+, which
   Ubuntu 24.04 doesn't ship (24.04 has 2.44).

If we'd rather keep the floor where it is, leave `build.target` on `safari15` and skip these features. Note that raising
the floor takes all three numbers (the plist, `build.target`, and the capability floor) plus the two API guards.

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
