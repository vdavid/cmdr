# Platform details

## How the values arrive

- `src-tauri/src/tauri_builder.rs` registers a tiny plugin (`cmdr-platform`) whose only job is a `js_init_script`, built
  by `apps/desktop/src-tauri/src/platform/frontend.rs`:
  `Object.defineProperty(window, '__CMDR_PLATFORM__', { value: '<os>' })` and
  `Object.defineProperty(window, '__CMDR_CAPABILITIES__', { value: Object.freeze({…}) })`. Tauri runs a plugin's init
  script in every webview the app creates, before the page's own scripts: the config's main window and each window the
  frontend opens later (viewer, Settings, the queue, the help and debug windows). So no window can render, or evaluate a
  module, without them.
- Both properties are read-only (`defineProperty` without `writable`, the capabilities frozen too), so nothing in the
  page can change its mind later.
- `getPlatform()` reads its global on every call and checks it names one of the three platforms. A missing or unknown
  value answers `FALLBACK_PLATFORM` (`'linux'`).
- `platformCapabilities()` / `hasCapability()` read theirs on every call, field by field: a flag holds only where the
  injected value says exactly `true`. A missing global, a missing field (an older or newer backend), or junk reads as
  "not offered", so a surface hides rather than shows by accident. `NO_CAPABILITIES` is the all-`false` set.
- `hooks.client.ts` calls `applyPlatformAttributes()` at module evaluation, which every window runs before its first
  route renders, so `:root[data-platform=…]` and `:root[data-overlay-title-bar]` selectors hold from the first paint.

Decision: an init script, not an IPC command. Why: several modules read these AT MODULE LOAD (the command registry's
`showInPalette` and `shortcuts` arrays, the settings registry), and an awaited `invoke` can't answer before they
evaluate. The other option, a `tauri-plugin-os` call, is async for the same reason, adds a dependency, and knows nothing
about Cmdr's features.

Decision: one platform value per process, typed in TS, and Rust's own `std::env::consts::OS` on the backend. Why: Rust's
names for the three targets are exactly the `Platform` values, so nothing maps or mirrors them, and the crate root
refuses every other target (`docs/guides/platform-seams.md` § Unknown targets) instead of silently reporting "linux".

## Capabilities

The frontend used to ask "is this macOS?" wherever a feature exists only there (Quick Look, Full Disk Access, the Finder
extension, …), which says nothing about Windows and hides each feature by accident rather than on purpose. Now the
backend says what exists, and a surface asks for exactly the thing it needs.

- **Source of truth**: `PlatformCapabilities` in `apps/desktop/src-tauri/src/platform/capabilities.rs`, one documented
  `bool` per OS-provided surface, named after the UI that asks. Each OS arm of the platform seam (`platform/macos.rs`,
  `platform/linux/mod.rs`, `platform/windows.rs`) answers every field as a struct literal, so a new field fails to
  compile until each OS has decided. The Linux arm's `imageIndexing` follows the `playwright-e2e` feature: the E2E build
  runs a fake image engine there (`cmdr-index`'s `media_index/scheduler/lifecycle.rs::platform_backends`), and nothing
  else does.
- **Every arm answers on purpose, Windows included.** The macOS-only surfaces with no other OS behind them are
  `localNetworkPermission` (a TCC prompt; Windows' firewall prompt is its own thing), `localAi` (the bundled
  llama-server is the macOS ARM64 build; the backend's `is_local_ai_supported` narrows it on the machine, so an Intel
  Mac still sees the option greyed out), `safeSaveFiles` (only NSDocument writes `.sb-` files), `appBundles` (no
  LaunchServices elsewhere; `cmdr-fs`'s `folder_names_make_packages` is the backend's matching per-OS fact),
  `fullScreen` (no way into full screen off macOS yet; flips on per OS with a native F11 entry), and `homebrewCask` (the
  macOS distribution). `globalShortcuts` is `true` on macOS and Windows (`RegisterHotKey`) and, on Linux, only on an X11
  session, read once at startup (`XDG_SESSION_TYPE`, else `WAYLAND_DISPLAY` unset): `global-hotkey` grabs keys through
  X11 only, so on Wayland the grab fires only inside XWayland windows. The backend asks it too before registering the
  hotkey (`downloads/runtime.rs`).
- **Typed in TS from Rust**: `ipc.rs` exports the struct with `.typ::<PlatformCapabilities>()`, so `bindings.ts` carries
  it like any command's type, and `Capability` is `keyof` it. No IPC command returns it; the init script is the only
  channel.
- **What asks**: the command registry (`needsCapability` on a command: every listing surface skips it and it gets no
  default key, `lib/commands/DETAILS.md` § "OS-feature commands"), settings (`needsCapability` on a definition or a
  searchable row drops it from the nav tree, search, Advanced, the MCP settings resource, and every section's
  `shouldShow`), the menu bar (`macos_only` rows, pinned to their capability per OS by
  `apps/desktop/src-tauri/src/menu/capability_rows_test.rs`), the nudges (`NUDGE_CAPABILITY` maps each offer to its
  own), Full Disk Access everywhere it shows, the F9 terminal slot, the Services selection push, and the overlay title
  bar.
- **What doesn't**: `isMacOS()` stays where the question really is the platform. Key glyphs and the combo grammar
  (`key-capture.ts`, the shortcut filter, the favorites tooltip), the default keymap's per-platform combos, the
  updater's macOS flow, and the old-macOS notice. ❌ Never copy: wording that differs per OS is one catalog value with a
  branch per OS (`../intl/DETAILS.md` § Platform variants), and `cmdr/no-platform-copy-branch` refuses an OS test that
  picks a string.

Decision: booleans, not a list of names or an OS-file-manager name. Why: a flag per surface is exhaustive in both
languages (a missing answer is a compile error in Rust and a type error in the demo's table), and the per-platform names
belong to copy, which an ICU `select` on the platform answers.

## The fallback

Contexts with no backend get `'linux'` and `NO_CAPABILITIES`:

- **Vitest**: the default test world is "not macOS, nothing offered". A test that needs another platform or a capability
  says so with `withPlatform()` / `withCapabilities()`.
- **A plain browser** (`vite dev` without Tauri): nothing answers IPC there anyway.
- **The browser demo** is the exception: its fake backend defines both globals itself from the visitor's user agent
  (`apps/desktop/src/lib/demo/platform.ts`), so a Mac visitor gets ⌘ shortcuts and the Mac app's surfaces, and everyone
  else Ctrl and none, as the keyboard under their hands expects. A page has no other signal; the sniff lives in the
  demo's backend, never in app code.

Why not macOS as the fallback: off macOS is the safe direction. A Mac-only surface (Quick Look, Full Disk Access, the
Finder extension) hiding in an odd context is harmless; one showing where nothing can answer it isn't.

## Test helpers

`withPlatform()` and `withCapabilities()` set the same globals the init script does and return the undo, so everything
that reads them follows at once, `key-capture.ts`'s formatting included. Mocking `isMacOS` instead flipped only the
importers the mock reached and left the rest on the host's answer.

- Bare: the switch holds until the next one. Each test file starts on the fallback (Vitest isolates files).
- Per test: `beforeEach(() => withPlatform('macos'))` (Vitest runs the returned undo as the teardown).
- Inside one test: `onTestFinished(withCapabilities('fullDiskAccess'))`. `withCapabilities()` offers exactly what it
  names, nothing else.
- As data, for pure code that takes a set (`NudgeContext.capabilities`): `capabilitiesWith('dockPin')`.
- Before the imports, for module-load reads: `vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))`, and
  `vi.stubGlobal('__CMDR_CAPABILITIES__', { quickLook: true })` (an absent field reads `false`). Hoisted code runs
  before any import, so it can't call the helpers and writes the globals itself. Decision: not
  `await vi.hoisted(async () => …import…)`, which reads better but breaks in a `.svelte.test.ts` (the Svelte transform
  nests the top-level `await`, and Vitest refuses a nested `vi.hoisted`; verified on Vitest 5.1.0, 2026-11-11).

Decision: `withPlatform('macos')` doesn't grant macOS's capabilities. Why: that would need a TS copy of each OS arm's
answers, which drifts from Rust, and a test about Full Disk Access should say it needs Full Disk Access rather than "a
Mac". A test of a Mac surface that also shows key glyphs sets both.

## Window chrome

`--titlebar-height` is the height of the overlay title bar: the main window's title bar sizes itself to it, and every
full-window backdrop starts below it so the window-drag region stays live under a dialog. Without one (the
`overlayTitleBar` capability, macOS only) the OS draws the title bar outside the webview, so `app.css` sets it to 0
under `:root:not([data-overlay-title-bar])` and a backdrop covers the whole window. The viewer's 35px override and the
queue window's in-webview title line follow the same capability.
