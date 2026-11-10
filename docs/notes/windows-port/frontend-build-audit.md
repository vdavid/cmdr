# Windows port: frontend, build, release, and services audit

Read-only audit of everything outside the Rust internals: the Svelte frontend, Tauri config, CI and release, the
website, api-server, and dashboard, and the test harnesses. Done 2026-11-04 on `main` at `7a199a9705`, by grep plus
reading the hot spots. Rust crates and app modules are covered by the sibling audits in this folder. The Linux build
plan (`docs/specs/linux-builds-plan.md`) and gap ledger (`docs/notes/linux-gaps-2026-08-10.md`) already cover a lot of
the shape; this doc points at them where they apply and only adds what Windows changes.

## Headline

- The frontend models the world as "macOS vs. everything else". Windows lands in the "else" bucket, which is mostly
  right for key formatting and mostly wrong for everything else (copy, chrome, capabilities, paths).
- The single biggest architectural win is a `#lib/platform` module fed by the backend: a three-valued platform, a
  capability set, and a root `data-platform` attribute. It replaces a UA sniff that opts out of the repo's own
  no-string-match rule, fixes two live Linux bugs, and is the seam every later Windows task plugs into. Clear win even
  if Windows never ships.
- The release pipeline, `latest.json`, the website's `dmgUrls`, the api-server's `/download` route, and the dashboard's
  funnel all hardcode "three macOS arches". Generalizing them to "N platform assets" once serves Linux and Windows
  alike.
- One cross-audit decision blocks the path work: what a Windows path looks like on the IPC wire. Recommendation below
  (forward-slash "Cmdr paths", conversion only at the Rust OS boundary).

## 1. Frontend (`apps/desktop/src/`)

### Platform detection

- `isMacOS()` lives in the shortcuts module, `apps/desktop/src/lib/shortcuts/key-capture.ts:22`, and sniffs
  `navigator.userAgent` for `mac`, with an `eslint-disable cmdr/no-error-string-match` (line 24). No backend platform
  command exists (no `platform`/`osInfo` in `lib/ipc/bindings.ts`, no `tauri-plugin-os`).
- 74 call sites in 28 non-test files. Heaviest: `lib/commands/sources/file-list.ts` (21),
  `lib/onboarding/onboarding-state.svelte.ts` (7), `key-capture.ts` (6), `KeyboardShortcutsSection.controller.svelte.ts`
  (4), `transfer-error-messages.ts` (4), `lib/commands/sources/main-window.ts` (4), `routes/(main)/startup-gates.ts`
  (3).
- Several sites evaluate `isMacOS()` AT MODULE LOAD (the command registry's `showInPalette`/`shortcuts` arrays, see the
  comment at `lib/shortcuts/registry-conflicts.test.ts:17`). So whatever replaces it must be synchronous at boot: a
  Tauri `initialization_script` that sets `window.__CMDR_PLATFORM__` before any module runs, not an awaited IPC call.
- What the sites actually ask is rarely "is this macOS": it's "is there Quick Look / a Finder extension / a Services
  menu / Full Disk Access / a reveal handler / a native Get Info", or "what's the OS file manager called". Those are
  capabilities, and the backend knows them (`cfg`-gated). Examples: `file-list.ts:377-517` (Show in Finder, Get Info,
  Quick Look, cloud commands), `FinderExtensionCard.svelte:35`, `RevealHandlerCard.svelte:72`,
  `coverage-cta.svelte.ts:116,129` (FDA), `app-state.ts:197` (Services menu), `updater.svelte.ts:232`.
- Vitest runs under happy-dom with a Linux UA, so the default test world is "non-mac"; mac-path tests stub the UA via
  `vi.hoisted` (`registry-conflicts.test.ts:22`, `key-capture.test.ts:22`). There's no Windows UA anywhere.

### Live bugs found (Linux today, Windows tomorrow)

- **Modal backdrops leave a 29px uncovered strip off macOS.** `--titlebar-height: 29px` (`apps/desktop/src/app.css:550`)
  is unconditional, but the main window renders its in-webview title bar only `{#if isMacOS()}`
  (`routes/(main)/+page.svelte:790`). Every full-window backdrop starts at `inset: var(--titlebar-height) 0 0 0`
  (`lib/ui/ModalDialog.svelte:715`, `CommandPalette.svelte:342`, `OnboardingWizard.svelte:360`), so on Linux the top
  29px of the webview stays live and unscrimmed under a dialog. Fix shape: `--titlebar-height: 0` under
  `:root:not([data-platform='macos'])`. Not fixed here (read-only audit).
- **`Ctrl+D` collision off macOS**: `favorites.open` is `⌃D` (`lib/commands/sources/main-window.ts:69`) and
  `file.duplicate` is `⌘D` (`file-list.ts:255`); `toPlatformShortcut` maps both to `Ctrl+D`, so favorites is dead in the
  file list. Known and parked: `lib/shortcuts/conflict-detector-registry.test.ts:17-25`.
- **api-server titles Linux reports "macOS Ubuntu 24.04 LTS"**: `macOsLabel` (`apps/api-server/src/report-cards.ts:140`)
  prefixes `macOS` to any `osVersion` not starting with it, but Linux clients send `/etc/os-release`'s `PRETTY_NAME`
  (`apps/desktop/src-tauri/src/platform.rs:20-29`). Windows would get "macOS Windows 11 …". XS fix.

### Keyboard shortcuts

- The canonical vocabulary is already platform-aware and well-tested: `formatKeyCombo` writes `⌘⌃⌥⇧` on macOS and
  `Ctrl+Alt+Shift+Super` elsewhere (`key-capture.ts:231-261`); display glyphs are a separate layer (`toDisplayShortcut`,
  line 172). Windows works with this as-is; `metaKey` becomes `Super` (the Win key), which Windows reserves almost
  entirely, so nothing should default-bind it.
- Registry defaults are written in macOS glyphs and converted mechanically by `toPlatformShortcut`
  (`key-capture.ts:358-404`): `⌘`→Ctrl, `⌃`→Ctrl, `⌥`→Alt, and a hack where `⌃` becomes Shift when `⌘` is also present.
  That mechanical mapping is the source of the `Ctrl+D` clash and can't express "on Windows this should be a different
  key" (Alt+F4 conventions, Alt+Enter = Properties, F-key-centric Total Commander habits). A few commands already
  hand-branch with `isMacOS() ? [...] : [...]` (`main-window.ts:322,334,393,401`, `file-list.ts:393,460,471`), which is
  the per-platform override pattern done ad hoc.
- The registry's no-conflicts test walks macOS only (`registry-conflicts.test.ts`); the other one walks the non-mac form
  with an exemption. Neither knows OS-reserved combos (Windows: Alt+F4, Alt+Space, Alt+Tab, Ctrl+Esc, Ctrl+Shift+Esc,
  Win+anything; GNOME: Super+anything).
- Alt alone focuses the native menu bar on Windows (and Alt+letter fires menu mnemonics if the Rust menu sets `&`
  accelerators). Defaults like `⌥F1`/`⌥F2`/`⌥F7`/`⌥Enter`/`⌥↑` become Alt combos, which is fine (they're Total
  Commander's own), but worth one manual check on a real Windows box.
- UI copy: only 9 of 5,782 English values carry a literal `⌘`/`⌥`/`⌃` (most shortcuts reach copy via placeholders,
  good). `ViewerStatusBar.svelte:62` already routes `⌘F` through `toPlatformShortcut`.

### Platform-specific copy (i18n)

- 230 of 5,782 English values (about 4%) mention `Finder`, `macOS`, `Mac`, `System Settings`, `Keychain`,
  `Full Disk Access`, `Quick Look`, `Spotlight`, or a mac modifier glyph (counted over values only, `@key` descriptions
  excluded).
- Today's mechanism is ad hoc key pairs (`*.mac.label` / `*.other.label`, five and ten keys) picked by `isMacOS()` at
  the call site (`file-list.ts:377,457,465`, `transfer-error-messages.ts:157`). ICU `select` is already supported and
  used (`{verb, select, …}` etc.), so the clean shape is: the intl layer auto-injects a `platform` argument into every
  message, and platform-varying strings become `{platform, select, macos {…} windows {…} other {…}}`. Translators see
  one string with its variants, and the `.mac`/`.other` pairs collapse.
- The MCP server's own instructions say "a keyboard-driven two-pane file explorer for macOS"
  (`apps/desktop/src-tauri/src/mcp/handler.rs:25`); Rust, but it's copy.

### Window chrome

- Main window: `titleBarStyle: "Overlay"`, `trafficLightPosition`, `hiddenTitle`, `macOSPrivateApi: true`
  (`apps/desktop/src-tauri/tauri.conf.json:14,23-29`). Child windows set the same from TS:
  `lib/settings/settings-window.ts:231-233`, `lib/file-operations/queue/queue-window.ts:146-148`,
  `lib/shortcuts/shortcuts-window.ts:93-95`, `lib/debug/debug-window.ts:82-84`, `lib/file-viewer/open-viewer.ts`. On
  Windows these options are ignored and `decorations: true` gives a native frame, the same as Linux today.
- Vibrancy: settings, queue, and debug windows call `setEffects({ effects: [Effect.Sidebar | UnderWindowBackground] })`
  (`settings-window.ts:249`, `queue-window.ts:167`, `debug-window.ts:95`), macOS materials. Tauri has Windows 11
  `Mica`/`Tabbed`/`Acrylic` effects; route the choice through the platform module (`windowMaterial('sidebar')`).
- Recommendation: native decorations on Windows for v1 (matches Linux, zero custom hit-testing). A custom title bar with
  in-webview caption buttons fights Windows 11 Snap Layouts on the maximize button and isn't worth it early. With native
  decorations, the Rust menu bar (muda) attaches inside each window under the title bar on Windows, as on Linux.
- `data-tauri-drag-region` regions (`routes/settings/+page.svelte:427`, `routes/queue/+page.svelte:526`) work on Windows
  too; under native decorations they're harmless.

### Paths

- `lib/path/canonical.ts` is the blessed path arithmetic, and it's POSIX-only by construction: `toCanonical` accepts
  `/…`, `~`, `~/…`, and `scheme://` and throws on anything else (`canonical.ts:24-35`), so `C:\Users\ada` or
  `C:/Users/ada` throws today. `isPlainFilesystemPath` is `raw.startsWith('/')` (line 53). `isPathOnVolume` is a
  case-sensitive component match (line 72).
- Outside `lib/path`, about 75 raw `split('/')` / `lastIndexOf('/')` sites across 40 files and 37 `startsWith('/')`
  checks. Many sit in virtual-volume utils where `/` is the protocol's separator and stays right (`mtp-path-utils.ts`,
  `adb-path-utils.ts`, `server-path-utils.ts`, `archive-paths.ts`); the rest (`transfer-dialog-utils.ts`,
  `transfer-dialog-logic.ts`, `navigate-and-select.ts`, `drag-drop-controller.svelte.ts`, `trash-undo.ts`, …) are local
  path math the `CanonicalPath` brand exists to prevent.
- `/Volumes` appears 76 times, but outside fixtures (`lib/dialog-gallery/fixtures/transfer-error.ts` has 25) nearly all
  are comments. No real logic keys on `/Volumes`; volume identity comes from the backend. Good.
- **The decision (cross-audit)**: what a Windows local path looks like over IPC. Recommendation: (a) the backend speaks
  forward-slash "Cmdr paths" to the frontend on every OS (`C:/Users/ada`, UNC as `//server/share/…`), converting to and
  from `\` only at the Rust OS boundary. Every existing frontend slash-arithmetic site stays correct; `canonical.ts`
  learns drive roots (`parentOf('C:/') === 'C:/'`) and `//server/share` roots; display converts to `\` in one formatter
  (`lib/utils/display-path.ts` already exists). The alternative, (b) native `\` paths on the wire, touches all ~110
  sites plus every Rust↔TS fixture. Case-insensitivity belongs on the volume record from the backend
  (`caseSensitive: bool`), and `isPathOnVolume` consults it. This needs agreement with the crates audit before anyone
  edits `lib/path`.

### Fonts and WebKit vs. WebView2 (Chromium)

- `--font-system: -apple-system, BlinkMacSystemFont, system-ui, sans-serif` (`app.css:474`): on Windows `system-ui`
  resolves to Segoe UI (Variable on Windows 11). Fine as-is. `--font-mono` ends in `Consolas, monospace` (line 475);
  optionally put `'Cascadia Mono'` before `Consolas` for Windows 11.
- `--font-scale` follows macOS Accessibility text size (`lib/text-size.svelte.ts`); Windows' equivalent is the "Text
  size" setting (`UISettings.TextScaleFactor`), which the backend would have to report. Verify whether WebView2 already
  applies it before building anything.
- WebKit-specific CSS is Chromium-compatible: `::-webkit-scrollbar` (17 sites) works in Chromium,
  `-webkit-backdrop-filter` (12) sits next to unprefixed `backdrop-filter` (16). The WKWebView workarounds
  (rAF-starvation fallback in `deferred-active-descendant.svelte.ts`, `prefers-reduced-transparency` via class in
  `display-accessibility.ts`, child-window-close timing in `lib/child-window-close.ts`, the ES5 boot guard) are harmless
  on Chromium.
- Scrollbars: `--spacing-scrollbar-width: 0px` assumes macOS overlay scrollbars (`app.css:463`). Windows draws classic
  ones; `FullListHeader` already measures the real width, so this is a check, not a task.
- HTML5 drag-and-drop: the app already avoids it in favor of pointer-driven reorders and `onDragDropEvent`
  (`menu-controller.svelte.ts:397`, `queue-drag.svelte.ts:7`, `drag-drop-controller.svelte.ts:602`), which behaves the
  same on Windows. Drag-OUT to Explorer is native (macOS promises) and belongs to the app audit.

### `cmdr-media://` and URLs

- The viewer, image search, and bulk-rename review build `cmdr-media://localhost/<token>` in exactly one place,
  `routes/viewer/media-view.ts:22-24`, and the CSP allows `cmdr-media:` in `img-src`/`object-src`
  (`tauri.conf.json:33`).
- **On Windows that URL doesn't load**: WebView2 serves Tauri custom schemes as `http://cmdr-media.localhost/<token>`
  (or `https://` with `app.windows[].useHttpsScheme`), verified in the Tauri 2.1 changelog (`tauri-utils` 2.1.0) and
  tauri-apps/tauri#9875, 2026-11-04. The app's own origin also becomes `http://tauri.localhost`. Fix: build the URL with
  `convertFileSrc(token, 'cmdr-media')` from `@tauri-apps/api/core` (it knows the per-OS form) inside `mediaUrl`, and
  add `http://cmdr-media.localhost` to `img-src`/`object-src`. The single-source design makes this a one-file change
  plus `viewer-media.spec.ts`'s CSP guard.

## 2. Tauri config, capabilities, bundling, updater

- One `tauri.conf.json`, no per-platform files. macOS-only keys sit in the shared file: `macOSPrivateApi`,
  `titleBarStyle`, `trafficLightPosition`, `hiddenTitle`, `acceptFirstMouse`, `bundle.macOS`
  (`tauri.conf.json:14-29,63-73`), and the `tauri` crate enables `macos-private-api` unconditionally
  (`src-tauri/Cargo.toml:142`). Tauri merges `tauri.macos.conf.json` / `tauri.windows.conf.json` /
  `tauri.linux.conf.json` automatically; moving the mac keys out is a clean, behavior-neutral win.
- `bundle.targets: "all"` (line 46): on Windows that's NSIS + MSI. Recommend NSIS only for v1 (cross-buildable, per-user
  install without admin, what the updater swaps); MSI later if enterprise asks. MSI builds only on Windows (WiX v3).
- WebView2: Windows 10 ships it on nearly all machines now, Windows 11 always. `webviewInstallMode`:
  `downloadBootstrapper` (default, 0 MB) or `embedBootstrapper` (about 1.8 MB, more robust offline-ish installs).
  Recommend `embedBootstrapper`. `offlineInstaller` (+127 MB) and `fixedVersion` (+180 MB) aren't worth it (verified on
  v2.tauri.app/distribute/windows-installer, 2026-11-04).
- NSIS `installMode`: `currentUser` (default, `%LOCALAPPDATA%`, no admin) is right for a file manager that updates
  itself silently.
- Windows app manifest (via `tauri-build`'s `WindowsAttributes::app_manifest`): add `longPathAware` (a file manager WILL
  meet paths over 260 chars; it also needs the `LongPathsEnabled` registry flag on the machine) and per-monitor-v2 DPI
  awareness. `Info.plist`/`Entitlements.plist` have no Windows twin today.
- Icons: `icons/icon.ico` (six sizes) and the `Square*Logo.png` set already exist and `pnpm icons` regenerates them
  (`scripts/regenerate-icons.sh:32`). Nothing to do.
- Capabilities (`src-tauri/capabilities/*.json`): portable. Two to verify on Windows: `opener:allow-open-path` with
  `**/*` (`default.json`) matching `C:\…` paths, and `$TEMP/drag-*.png` resolving to `%TEMP%`. `updater:default` and
  `process:allow-restart` are granted.
- Updater:
  - `latest.json` platform keys follow `OS-ARCH`: `windows-x86_64`, `windows-aarch64`. Tauri also reads
    `{os}-{arch}-{installer}` (`windows-x86_64-nsis`) first when present; verify before relying on it.
  - With `createUpdaterArtifacts: true` (`tauri.conf.json:47`) Windows emits `Cmdr_<v>_x64-setup.exe` + `.sig` (and the
    MSI pair). Tauri quits the app before running the installer on Windows (verified on v2.tauri.app/plugin/updater,
    2026-11-04), so the "staged, restart to apply" UX doesn't map 1:1.
  - **Security gap**: off macOS the frontend uses the plugin's fused `check()` + `downloadAndInstall()`
    (`lib/updates/updater.svelte.ts:390-430`), which skips the advisory list, the managed-install check, and the custom
    trust verification the macOS flow runs (comment at line 429). Fine for an unadvertised Linux alpha; not fine for a
    Windows release. Route Windows through the same Rust-side check (`apps/desktop/src-tauri/src/updater/`,
    `cmdr-update-trust`) and use the plugin only for the installer hand-off. Overlaps the app audit.
- Code signing, current state (checked 2026-11-04):
  - **Azure Artifact Signing** (renamed from Trusted Signing, GA in January 2026): public-trust certificates for
    organizations in the USA, Canada, the EU, and the UK; individuals only in the USA and Canada. So it fits through
    Rymdskottkärra AB, not as David personally. No minimum company age. About $9.99/month (Basic, 5,000 signatures).
    Needs a paid Azure subscription and org identity validation (sources: Microsoft Learn Artifact Signing quickstart
    and FAQ, Q&A thread 5977141, 2026-11-04).
  - Tauri wires it through `bundle.windows.signCommand` with `trusted-signing-cli` / `artifact-signing-cli` (verify the
    current tool name and that it's alive before pinning). In CI it can authenticate with GitHub OIDC federated
    credentials (`azure/login`), so no long-lived signing secret sits in GitHub, which matches the repo's "updater key
    never reaches GitHub" stance.
  - Alternatives: an OV certificate on a hardware token or cloud HSM (Azure Key Vault + `relic`), pricier and more ops.
    EV no longer buys instant SmartScreen reputation, so it isn't worth the premium.
  - Unsigned is a non-starter: Windows 11 Smart App Control blocks unknown unsigned apps outright, and users can't allow
    one app, only turn the feature off.

## 3. CI, check runner, release

### CI (`.github/workflows/`)

- `ci.yml` (2,054 lines) runs everything on `ubuntu-latest` except `desktop-rust-macos`, which calls `macos-lane.yml`
  (`runs-on: macos-26`, line 24: clippy, rustdoc, tests, module cycles). That file is the template for a
  `windows-lane.yml`.
- A Windows lane worth having from day one, in order of value:
  1. `cargo clippy --target x86_64-pc-windows-msvc` (and `cargo check` on the workspace) on `windows-latest`, native.
     Cross-checking from Linux/macOS works for pure-Rust crates but stalls on any build script that compiles C for MSVC
     (`cc`-built SQLite and friends) without `cargo-xwin`; native is simpler. Start it non-blocking (outside `ci-ok`),
     scoped to members that build on Windows, and promote to blocking once green. Rust cache keys are already per OS.
  2. The portable crates' tests (`cargo nextest`) once they compile: the cheapest way to catch path-separator and
     case-insensitivity assumptions.
  3. Later, a Playwright smoke subset (see § 5).
- Member selection already supports this: each crate declares `[package.metadata.cmdr] platforms = […]` and the check
  runner filters via `WorkspaceMember.BuildsOn` (`scripts/check/checks/cargo-workspace.go:83`); `cargoOSName` maps
  `runtime.GOOS` (`cargo-workspace.go:310`), and Go's `windows` already equals cargo's spelling. Adding Windows is
  declaring `"windows"` where it applies, not new Go.
- `cargo deny` vets only `aarch64-apple-darwin` and `x86_64-apple-darwin` (`deny.toml:9`, "add a triple the moment we
  ship one"). Add `x86_64-pc-windows-msvc` with the first Windows release; expect a new batch of `windows-*` crates in
  third-party notices and the SBOM. `desktop-third-party-notices.go:371` keys its `cargo-about` binary by `GOOS/GOARCH`,
  so it needs a `windows/amd64` asset only if that check ever runs on a Windows host.

### Check runner (`scripts/check/`)

- Several checks gate on `runtime.GOOS != "darwin"` (E2E Playwright `desktop-svelte-e2e-playwright.go:72`, Miri, disk
  images, Finder extension, macOS availability and symbol floor); those correctly skip elsewhere. Seven files shell out
  to `bash`/`sh` (Linux container, stack lease, module cycles), and the entry point is `scripts/check.sh`. On a GitHub
  Windows runner, call the Go binary directly with `shell: bash` (Git Bash) for the few steps that need it; making
  `pnpm check` itself run on a Windows dev box isn't needed (David develops on macOS).
- `desktop-rust-cfg-gate` guards macOS-only imports (`desktop-rust-cfg-gate.go:14`), but nothing guards the opposite
  assumption: the app crate has 118 `#[cfg(not(target_os = "macos"))]` blocks, most of which mean "Linux" and would
  silently compile into the Windows build. A scanner that flags `not(target_os = "macos")` and steers to an explicit
  `linux` / `unix` / `windows` (with a shrink-wrapping allowlist) turns intent into something checkable. Win today: it
  documents which fallbacks are real Linux code vs. genuinely portable.
- `desktop-rust-clippy-linux` lints the Linux target from a Mac via Docker. There's no local equivalent for Windows (no
  Windows containers on macOS); `cargo xwin clippy` could fill it, but CI is enough to start. Verify `cargo-xwin` is
  alive before adding it.
- Dev scripts: `apps/desktop/package.json` scripts use POSIX shell (`CMDR_DEMO_BUILD=1 vite …`,
  `$(rustc -vV | grep host …)`), and `pnpm` runs them with `cmd` on Windows unless `script-shell` points at bash. Only
  matters if someone builds on Windows locally; CI can set `shell: bash`.
- `apps/desktop/scripts/instance-id.ts:132,159` (`computeAppDataDir` / `computeAppCacheDir`) has a `darwin` branch and
  treats everything else as XDG. Windows needs `%APPDATA%\<identifier>` for data (what Tauri's `app_data_dir` returns)
  and `%LOCALAPPDATA%\<identifier>` for the drive index. Keep big caches out of Roaming. The Rust mirror
  (`resolved_app_data_dir`) must match. `tauri-wrapper.ts:279-281` forces `universal-apple-darwin` only on darwin, which
  is right.

### Release (`release-pipeline.yml`, `scripts/release-*`)

- Hardcoded to three macOS arches throughout: `compile` and `sign` matrices (`release-pipeline.yml:243-260,348-373`),
  `vet-update-archives` on `macos-26` (line 985), `publish` reading exactly three `.app.tar.gz.sig` files and writing
  three `darwin-*` keys plus a top-level `dmgSizes` (lines 1110-1150), `scripts/release-finish/state.go:62`
  (`arches = aarch64, x64, universal`), the SBOM job (`for pair in aarch64-apple-darwin …`, line 919), checksums "for
  the DMGs and the pkg", and `bump-tap` (Homebrew).
- What a Windows release needs: a `windows-latest` build+sign job (x86_64 first; `windows-11-arm` runners exist for
  aarch64, verify availability for this repo), Artifact Signing of the exe and the NSIS installer, the minisign updater
  `.sig` in both `ci` and `local` signing modes (`release-finish` signs whatever the asset list names), a
  `windows-x86_64` entry in `latest.json`, an Authenticode verify step in `vet-update-archives` (`signtool verify /pa`
  on Windows or `osslsigncode verify` on Linux), checksums, SBOM, provenance attestation, and a winget manifest bump as
  the `bump-tap` twin.
- The elegant move, shared with the Linux plan's Part 1 and Part 5: generate one release asset manifest (os, arch, kind,
  file name, updater key) early in the pipeline, and have `publish`, `release-finish`, `vet-update-archives`, checksums,
  and SBOM all iterate it. Then adding Linux or Windows is a matrix entry plus manifest rows, and the "latest.json names
  an asset that doesn't exist" guard (line ~1155) keeps working unchanged.

## 4. Website, api-server, dashboard

- Website: `apps/website/src/lib/release.ts:10-24` exports `dmgUrls` / `dmgSizes` for three mac arches from
  `latest.json`'s `dmgSizes`. The Linux plan (Part 3) designs OS detection in `Layout.astro`, per-OS data attributes,
  and a platform-aware `Download.astro`; build that once for `{macos, linux, windows}` and Windows costs a few rows. The
  roadmap (`apps/website/src/lib/roadmap.ts`) lists "Add Windows and true Linux support" for next year.
- api-server `/download/:version/:arch` accepts only `aarch64 | x86_64 | universal` and always redirects to a `.dmg`
  (`apps/api-server/src/telemetry/telemetry.ts:378,553-556`). Add `/download/:version/:os/:arch` (keep the old route as
  the macOS alias, since published links point at it).
- The funnel's UA classifier treats any `Windows`/`Linux`/`X11` UA as a provably impossible `bot`
  (`apps/api-server/src/user-agent.ts:17`, mirrored in `apps/analytics-dashboard/src/lib/funnel.ts:58` and the
  "Downloads by client" pie in `AcquisitionTrendsSection.svelte`). Correct today; wrong the day a non-mac artifact
  exists. Make it artifact-aware: a UA is "impossible" only for an artifact of another OS.
- Telemetry rows carry `os_version` and `arch` but no `os` (`heartbeat.ts:18`, `feedback.ts:56`, error and crash
  reports). The OS is only implicit in the free-form `osVersion` string. Add an explicit `os` field (D1 migration +
  client), so dashboards can split by platform without parsing strings (and without breaking the no-string-match
  spirit). Crash cards are macOS-shaped (`signal`, `os_exception`, "**macOS says**" in `report-cards.ts:277`); a Windows
  crash reporter would need its own fields (exception code, minidump), which is an app-audit item.
- Licensing (`apps/api-server/src/licensing/`) is platform-agnostic: device sets are opaque hashes. The client's device
  hash source is an app-audit item. Pricing per platform is a business question (`docs/business/README.md`).

## 5. Tests

- Playwright E2E talks to `tauri-plugin-playwright` over a Unix socket. The plugin (0.4.1) already falls back to TCP on
  non-Unix (`server.rs:27`, `cfg(unix)` around the socket branch, TCP default port 6274), so Windows needs the harness
  to pass a random high TCP port instead of a socket path; no fork. The E2E checks require `darwin` or the Linux Docker
  path today (`desktop-svelte-e2e-playwright.go:72`).
- `test/e2e-playwright/helpers/core.ts:31,38`: `CTRL_OR_META` and `LOCAL_VOLUME_NAME` are mac-vs-Linux binaries
  (`'Macintosh HD'` / `'Root'`); a Windows volume is `C:` or its label. Fixtures assume `/tmp/cmdr-e2e-fixtures`.
- Vitest: introduce a `withPlatform('windows' | 'linux' | 'macos')` test helper on top of the platform module instead of
  UA stubs, and run the shortcut-vocabulary, conflict, and `lib/path` suites under all three. Running Vitest on a
  Windows runner isn't worth it; the logic is platform-parameterized, not host-dependent.

## 6. Proposed tasks

Sizes per the board (XS < 200k tokens one agent; S 200–400k; M one agent + ≤ 3 subagents; L 4–15 agents). "Win anyway"
means worth doing even if Windows never ships.

1. **`#lib/platform` module** (S, win anyway). Backend-sourced `Platform = 'macos' | 'linux' | 'windows'` injected
   synchronously via `initialization_script`; `isMacOS()` moves there and stops sniffing the UA (drops the
   no-string-match opt-out); `data-platform` on `<html>`; `--titlebar-height: 0` off macOS (fixes the backdrop strip);
   `withPlatform` test helper. Shim the old `isMacOS` export so call sites migrate in task 2.
2. **Capabilities instead of OS checks** (S, win anyway). Backend returns a typed capability set (Quick Look, Finder
   extension, Services menu, Full Disk Access, reveal handler, native Get Info, cloud providers, OS file-manager name);
   migrate the 74 `isMacOS()` sites in 28 files to ask the capability. Depends on 1.
3. **Per-platform default keymap** (S, win anyway). Registry entries declare explicit per-platform overrides where the
   mechanical mapping is wrong; retire the `⌃`→Shift hack in `toPlatformShortcut`; fix `Ctrl+D`; run the no-conflicts
   test for all three platforms; add an OS-reserved-combo test (Windows and GNOME lists). Depends on 1.
4. **Platform-variant copy** (M for the English and mechanism, win anyway, Linux needs it). Intl auto-injects
   `platform`; migrate the ~230 values and the `.mac`/`.other` key pairs to `select` or neutral wording; teach the i18n
   checks. Translation of the changed strings is a separate translator-agent pass per the i18n guide (likely L across 13
   languages). Depends on 1. David reviews the English.
5. **Small fixes bundle** (XS, win anyway). `macOsLabel` prefixing non-mac OS names; MCP instructions "for macOS"
   wording; Cascadia Mono in `--font-mono`.
6. **Path wire format + `lib/path` drive and UNC roots** (S, Windows-only, but the lint half is a win anyway). After the
   cross-audit decision: `toCanonical`/`parentOf`/`isPlainFilesystemPath`/`isPathOnVolume` learn `C:/` and
   `//server/share`, volume `caseSensitive` flag, `\` in the display formatter, plus an ESLint rule steering raw local
   `lastIndexOf('/')` / `split('/')` to `lib/path` (virtual-volume utils allowlisted).
7. **`cfg(not(target_os = "macos"))` scanner** (XS, win anyway). Extend `desktop-rust-cfg-gate` or add a sibling;
   shrink-wrapping allowlist seeded with today's 118 app sites; migrating them is the Rust audits' job.
8. **Windows CI lane, non-blocking** (S, Windows-only but cheap insurance). `windows-lane.yml` mirroring
   `macos-lane.yml`: native clippy and check for Windows-capable members, then their tests; `platforms` metadata gains
   `"windows"` where true; promote to blocking when green.
9. **Tauri config split** (XS, win anyway). Move mac keys to `tauri.macos.conf.json`, gate the `macos-private-api`
   feature to macOS, explicit per-OS `bundle.targets` (Linux plan wants this too); add `tauri.windows.conf.json` with
   NSIS, `embedBootstrapper`, `currentUser`, and the app manifest (`longPathAware`, DPI).
10. **`cmdr-media` URL + CSP portability** (XS, Windows-only). `mediaUrl` via `convertFileSrc`, CSP gains the
    `.localhost` origin, guard spec updated.
11. **`instance-id.ts` Windows dirs** (XS, Windows-only). `win32` branches for data (Roaming) and cache (Local), in
    lockstep with the Rust mirror.
12. **Release asset manifest** (M, win anyway: it's also the Linux plan's Parts 1 and 5). One manifest drives `publish`,
    `latest.json`, `release-finish`, `vet-update-archives`, checksums, SBOM, and website sizes; replaces the top-level
    `dmgSizes` with a per-asset block (keep `dmgSizes` for one release for old website builds).
13. **Windows release build + Artifact Signing** (L, Windows-only). Azure account and org validation for Rymdskottkärra
    AB (David), OIDC-federated `azure/login`, `signCommand`, Windows build/sign job, minisign `.sig` in both signing
    modes, `windows-x86_64` manifest row, Authenticode vetting, `deny.toml` triple, notices and SBOM, winget bump job.
    Depends on 9 and 12. Security-critical: David reviews the signing flow.
14. **Updater trust parity off macOS** (S, Linux benefits too). Windows (and Linux) run the Rust-side check, advisory
    list, and managed-install logic, using the plugin only for the installer hand-off; UI handles "the app quits to
    install". Overlaps the app audit; one owner.
15. **Downloads and analytics for N platforms** (M, win anyway for Linux). `/download/:version/:os/:arch`,
    artifact-aware UA classification, explicit `os` field through heartbeat, feedback, and reports (D1 migration),
    dashboard split by OS, and the Linux plan's Part 3 website work built for three OSes at once. David reviews the
    download page copy and design.
16. **Windows E2E smoke** (S, Windows-only). TCP transport wiring for `tauri-plugin-playwright`, three-way helpers,
    Windows fixture root, a small spec subset on `windows-latest`. After the app runs on Windows.

Suggested order: 1, 5, 9, 7 (cheap wins), then 2, 3, 12 (structural wins), then 8 and the path decision (6), then 4 and
15 (copy and services), and 10, 11, 13, 14, 16 once the app compiles for Windows.
