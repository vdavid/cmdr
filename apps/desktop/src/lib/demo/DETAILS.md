# Browser demo: details

## What it is

A playable demo on getcmdr.com/demo: the real Svelte frontend on a mocked backend, so a visitor tries the keyboard flow
without installing anything. Issue #302 holds the research and David's decisions (option (b): the real listing engine in
WASM; lives at `/demo`; phones get the same page; placeholders for the file tree and shortcut map until David reworks
them; a later landing-page embed in an iframe, loaded on click).

Every IPC call goes through `invoke()` from `@tauri-apps/api/core`, which talks to `window.__TAURI_INTERNALS__`. The
demo replaces that object (Tauri's own `mockIPC` and `mockWindows`, without their event emulation), and nothing in the
app knows it runs in a browser.

## Isolation

- The app-side footprint is exactly: the first import in `hooks.client.ts`, the `__CMDR_DEMO_BUILD__` define plus the
  two aliases and the demo `fs.allow` in `vite.config.js`, the global in `app.d.ts`, and `vitest.config.ts`'s `false`
  for the flag plus the same `boot-absent.ts` alias (so a test importing `hooks.client.ts` loads no demo module).
- Why no `if (demo)` branches: a branch makes the demo stop showing the app it advertises, and every one is upkeep on
  the app side. Everything the demo needs goes through a seam the app already has: an IPC answer, a store seed (the
  shortcut map, onboarding done), a typed refusal.
- Enforced by `cmdr/demo-isolation` (`apps/desktop/eslint-plugins/demo-isolation.js`): outside `src/lib/demo/`, no
  `__CMDR_DEMO_BUILD__` (except its declaration in `app.d.ts`) and no import of `#lib/demo` or a relative `lib/demo/`
  path (except `hooks.client.ts`'s import of the boot module). It has no opt-out on purpose.

## Boot order

- `hooks.client.ts` imports `#lib/demo/boot.js` as its FIRST import, so ESM evaluates it before any other app module.
  `invoke` reads the internals at call time, so the mock only has to exist before the first call.
- `boot.ts` installs itself at module evaluation behind `if (__CMDR_DEMO_BUILD__)`, so a test that imports it directly
  (not through the aliased `#lib/demo/boot.js`) doesn't install the mock.
- Decision: a static import, never `if (__CMDR_DEMO_BUILD__) await import(...)`. The app bans top-level `await`
  (`no-restricted-syntax`) because WebKit can run an importer before an awaiting module finishes, which would let app
  modules call `invoke` before the mock exists. The demo runs in Safari too.

## Keeping the demo out of the app

- `vite.config.js` aliases exactly `#lib/demo/boot.js` to `boot-absent.ts` (an empty module) in every build without
  `CMDR_DEMO_BUILD=1`, the dev server included.
- Why the alias and not just the guard: with the guard alone, Rolldown dropped `installDemoBackend` but kept
  module-level data the demo modules compute (the whole demo file tree was in a normal `build/`'s `app.*.js`, verified
  on Vite 8.3.5 / Rolldown, 2026-11-04, by grepping `build/` for `/Users/you`). The alias keeps every demo module out of
  the graph whatever it computes.
- Verify after touching the boot path: `pnpm build` in `apps/desktop`, then grep `build/` for `__cmdrDemo` and
  `/Users/you`. Both must be absent.

## Backend and webviews

Tauri's model is one Rust process and many webviews: every window's `invoke` reaches the same state, and `emit` reaches
every window. The demo keeps that shape, so secondary windows share its state, as on desktop.

- `backend.ts` (`createDemoBackend`): the answer tables, the in-memory stores (`plugins.ts`), the WASM engine, and the
  event fan-out. Exactly one per page, in the top frame, exposed as `window.__cmdrDemoBackend`.
- `webview.ts` (`installWebview(backend, label)`): one frame's `__TAURI_INTERNALS__`. It answers what belongs to its own
  window (`plugin:window|inner_size`, focus, title, size limits) from its own `window`, keeps its own event listeners,
  and forwards everything else to the backend.
- Events: `plugin:event|listen` / `unlisten` stay in the webview (the listener's callback lives in that frame);
  `plugin:event|emit` and `emit_to` go to the backend, which hands them to every attached webview. A webview delivers to
  each listener whose target the event reaches (`emit` reaches all; `emit_to(label)` reaches `Any` listeners and that
  label's), with `{ event, id, payload }`, as Tauri does. Why not the mocks' `shouldMockEvents`: it keeps listeners per
  frame with no way to reach another frame, and has no `emit_to`.
- The wire is JSON both ways (`overTheWire`): no object is shared between the app and the backend, so the app mutating
  an answer (a store value, a listing row) can't change the backend's state, as with Tauri's serialization.
- `boot.ts` picks the role: a frame whose parent holds `__cmdrDemoBackend` is a secondary window and installs only a
  webview, labeled by its `window.name`; any other frame (top, or framed by a page without a backend, or by a
  cross-origin page, where reading the parent throws) creates the backend and installs the `main` webview.
- A window-plugin question about ANOTHER window (main asking for Settings' size) carries that window's `label`, so the
  webview forwards it to the backend instead of answering for itself.

## Secondary windows

- `windows.ts` (`createWindowHost`, owned by the backend) answers the app's window plumbing:
  `plugin:webview|create_webview_window` opens an in-page panel (a same-origin `<iframe name="<label>">` of the route
  under the demo's base, with the window's traffic lights over it), and `get_all_windows`, `close`, `destroy`,
  `set_focus`, `set_effects`, `start_dragging`, `internal_toggle_maximize`, plus Cmdr's `close_child_window` and
  `raise_child_window`, work against those panels. Opening one that's already open raises it, through the app's own
  `getByLabel` + `raiseChildWindow` path.
- `settings`, `shortcuts`, `queue`, and every `viewer-*` label open (`isOpenable`; the viewer's labels are
  `viewer-<timestamp>`, several at once). The queue runs on the write engine's commands and events (`list_operations`,
  `operations-changed`, pause, resume, cancel, reorder, the tree view). Every other label (`debug`) gets Tauri's
  refusal: the creation promise rejects with a string, which reaches the opener as `tauri://error`, so the app's own
  failure handling runs and no panel appears.
- The viewer runs on the WASM engine's viewer (`crates/cmdr-listing-wasm/src/viewer.rs`), so the route is unmodified:
  its commands route to the engine like the listing's, its native-menu plumbing (`viewer_setup_menu`,
  `viewer_set_word_wrap`, `viewer_set_search_input_focused`, `viewer_set_history_state`) is silent, and a closing panel
  calls the engine's `closeViewerWindow` (the app's window-destroyed hook), freeing its sessions and media tokens. An
  engine event with a target (`viewer-media-load`) reaches that window only. The engine's `demo://open-url` notice goes
  through the same `openInBrowserTab` as every other link (§ Links); `demo://media-token-dropped` never reaches a
  listener.
- Verified in headless Chromium (2026-11-04, local `build:demo`): F3 on `notes.md` renders its Markdown; text mode, ⌘F
  search with its match count, hex, and a 38 MB generated `Cmdr.dmg` in streaming text all work, and Esc closes the
  panel, with `__cmdrDemo.unanswered` empty throughout.
- Keys: Esc is the window's own (Settings calls `close_child_window`); ⌘W is the native menu's Close Window, which the
  browser keeps for its tab, so the panel takes ⌃W, the demo's browser-safe stand-in for ⌘W (`browser-shortcuts.ts`).
- Focus: a native window keeps its focused element while another window is key, but a page has one focus, so the host
  remembers what had focus in the main window when a panel took it and puts it back when the last panel closes. It also
  sends `tauri://focus` / `tauri://blur` / `tauri://destroyed` to the windows involved, as Tauri does.
- Look: plain app tokens (`--color-bg-primary`, `--radius-window`, `--shadow-lg`, `--z-modal`), injected once as a
  `<style>` under `.cmdr-demo-window`. Decision: no title bar of the demo's own. Every Cmdr window is an overlay window
  with a hidden title, so the panel is the page edge to edge, its lights over the page where its `trafficLightPosition`
  puts them (§ Traffic lights), and the page's own drag strip (`data-tauri-drag-region`) moves it. Why: a demo bar
  pushed the page down and put the lights in a spot no real window has (David saw them oversized in Settings). The strip
  works because `drag-regions.ts` ports Tauri's injected `drag.js` into every frame: a press there sends
  `start_dragging` (plus the press's screen position, which a page needs), and a double click released in place sends
  `internal_toggle_maximize`, the panel's zoom. The panel follows the mouse in screen coordinates, listening in both its
  frame and the page, since a held button's moves can stay with the frame. It opens centered, sized to the window's
  width and height clamped to the viewport (fits at 960×600), and can't be dragged past an edge. Opening, dragging, and
  zoom all keep it under the menu bar (`--cmdr-demo-window-top`), as macOS keeps windows under its own.
- Verified in headless Chromium (2026-11-04): ⌘, opens Settings; Esc, ⌃W, and the red light close it, and the cursor
  keys move the main pane's cursor right after; dragging moves and clamps it; a "Size colors" change in Settings
  recolors the main window's sizes through the fan-out. Keyboard shortcuts opens from the command palette, closes with
  Esc, and its "Edit shortcuts" link opens Settings at that section as a second panel (a child window opening another
  through the shared backend).
- The production CSP (`apps/website/nginx-demo-security-headers.conf`) needs `frame-src 'self'` for the panels: any
  stricter `frame-src` blocks their iframes.
- Known gap: clicking the main window while a panel is open moves DOM focus there but sends no `tauri://focus` /
  `tauri://blur`, and the panel stays on top (on desktop, main would become key and come forward). Fixing it means
  listening for `focusin` outside the panels and lowering them; nothing in the app reacts badly to the missing events
  today.

## The menu bar

- **What**: a strip at the top of the page (`menu-bar.svelte.ts`, first in `<body>`, so the app's flex column starts
  under it) that draws the macOS menu bar the desktop builds and sends what its items send. It sets
  `--cmdr-demo-window-top` on `:root` to its height, for the demo chrome that sits on the main window. The app's status
  corner and top toast stack sit against the page's top edge (the window's, on the desktop), so `menu-bar-style.ts`
  moves both down by it; without that the bar covers the copy's progress chip and eats its click (the website spec's
  queue cell caught it).
- **Decision**: the bar is the desktop's own, through WASM. `crates/cmdr-menu` holds the bar as data and the enabled
  fold; `cmdr-listing-wasm`'s `DemoMenuBar` stores the menu-state pushes the app already sends the native menu
  (`activate_window_menu`, `set_commands_refused_over_dialog`, `update_menu_accelerator`, `update_sort_menu`,
  `update_view_mode_menu`, `sync_menu_show_hidden`, the live labels) and answers the bar and a click from them
  (`crates/cmdr-listing-wasm/DETAILS.md` § The JS API). **Why**: a TS copy of the bar would drift on the first new item,
  and a click that sent anything other than the native event would need its own handler in the app.
- **Words**: each row's catalog key through `getMessage`, its `{token}`s replaced literally, as the desktop's
  `menu_t_with` does. The app menu's title is the literal `Cmdr`.
- **Language**: the closed strip redraws the moment the top frame's language lands, from a `$effect` over the catalog's
  locale-version rune (the file is `.svelte.ts` for it), as `followWindowTitle` keeps a native title in step. Decision:
  not on the `set_ui_language` push the desktop's menu rebuilds on. Why: Rust reads its own catalog synchronously, but
  the push reaches the demo before the webview's new catalog chunk loads, so a redraw on it would draw the old words. An
  open menu is left alone; its next open redraws it.
- **Shortcuts**: the spec's accelerators in a Mac's glyphs, or the pushed rebind, so the browser-safe map
  (`browser-shortcuts.ts`) shows: Tab > New tab reads ⌃T.
- **Clicks**: `execute-command`, `settings-changed`, `view-mode-changed`, and `menu-sort` go to the main window, as
  `handle_menu_event`'s `emit_to("main")`. With a panel in front, Close tab closes it and an edit goes to its frame
  (`document.execCommand`, best effort: browsers refuse a scripted paste).
- **Keys**: ⌃F2 focuses the bar (macOS's own shortcut for it); ←/→, ↑/↓, Enter, Space, Home/End, and Esc walk it as a
  Mac menu does; every key the bar or an open menu has is kept from the app (`stopPropagation` in a window capture
  listener). A greyed row never picks. OS rows (Services, Hide, Quit, Undo, Minimize) draw greyed.
- **Look**: the app's glass tokens (`app.css` § Frosted-glass material), sized against macOS 27's bar and an open menu
  (`menu-bar-style.ts`). A draft David reviews.
- Verified in headless Chromium against `pnpm build:demo` (2026-11-04): View > Show hidden files lists `.zshrc` and the
  check follows; View > Command palette opens it; Go > Home runs; View > Sort by > Size checks Size once the sort lands;
  Tab > New tab adds a tab and reads ⌃T; Cmdr > Settings opens its panel, File's items grey while it's in front, and Tab
  > Close tab closes it; the keyboard path from ⌃F2 reaches the palette; nothing lands in `unanswered`.

## Right-click menus

- **What**: the native popups the app opens (`show_file_context_menu`, the breadcrumb's, a tab's, the date header's Sort
  by, the function key bar's, the viewer's text, a queue row's, a servers hub host's), drawn as HTML macOS 27 menus
  (`context-menu.ts`) with the bar's look and keys. The app already cancels the browser's own menu
  (`#lib/context-menu-ownership.ts`), so a right-click reaches the same `show_*` command it does on desktop.
- **Decision**: the rows are the desktop's own, through WASM. `crates/cmdr-menu`'s `context` holds every popup's rows
  and what a pick asks for; `cmdr-listing-wasm`'s `context_menu.rs` answers each `show_*` command with them and stashes
  what the menu went up over, as `commands/menu.rs` stashes it in `MenuState` (`crates/cmdr-listing-wasm/DETAILS.md` §
  The JS API). **Why**: the same as the bar's; a TS copy drifts on the first new item, and a pick that sent anything but
  the native event would need its own handler in the app.
- **Routing**: the backend asks `menu.opensPopup(cmd)` before the menu-state pushes. Each webview passes its label and
  where its pointer last went down (`InvokeOrigin`, tracked in `webview.ts`), so the backend draws the menu in the TOP
  frame at the pointer, offset by the panel's iframe for a secondary window, or at the `anchor` the app names for a
  keyboard-opened menu (which then starts on its first enabled row). Top frame on purpose: a native popup isn't clipped
  to its window, and one renderer serves every window. The command answers `null` once the menu is up, as the native one
  returns while its menu tracks.
- **Picks**: `menu.contextClick(id)`, then `menu-click.ts` sends it: an event to `main`, `queue`, or the viewer the menu
  went up over, or the `add_favorite` command (the desktop calls it directly, for its add gate). A row the last popup
  didn't offer enabled does nothing, in the engine as in the page.
- **Left out**: every OS-filled group (Open with, Share, the tag colors, the provider's actions, Services): a browser
  has no OS to ask. The image-search group stays hidden (no media index).
- **Keys**: ↑/↓, Home/End, → into a submenu, ← or Esc out, Esc closes, Enter / Space pick, Tab closes; every key is kept
  from the app while the menu is up. The header line is a `presentation` row, never a stop, and names the menu for a
  screen reader (`aria-label`). A click outside, or the page losing focus, closes it.
- **Shared with the bar**: `menu-surfaces.ts` (an open menu and its submenus: drawing, placement, highlight, the walk),
  `menu-click.ts` (performing a pick), and `menu-bar-style.ts`'s `MENU_SURFACE_STYLE`, injected once per document.
- Verified in headless Chromium against `pnpm build:demo` (2026-11-04): right-clicking `Documents` draws the folder menu
  named by its header, with the live shortcuts (F5, ⌥⌘C) and no OS-filled group; ↓ lands on Selection, hovering it opens
  the submenu beside it, and Selection > Select all selects the whole pane; no page error.

## Traffic lights

On desktop the OS draws the close, minimize, and zoom buttons over each window's overlay title bar, at the
`trafficLightPosition` the app gives that window, and the app only leaves them room. In a browser nothing draws them, so
`traffic-lights.ts` does, from the demo's own DOM: the main window's as a fixed group at its top-left, each panel's
absolutely placed over its frame (`windows.ts`).

- Placement has one source, the app's own numbers: the main window's `trafficLightPosition` is read from
  `tauri.conf.json` (imported), and a panel's from the `create_webview_window` options its opener sends (Settings (20,
  29), Keyboard shortcuts (14, 18), viewer and queue (9, 17), as `LogicalPosition` serializes, `{ x, y }`).
  `lightsInset` turns one into the close button's corner: `x` as is, `y - 9` for the top (tao grows the title-bar
  container to the button's frame plus `y`, and AppKit pins the button to the container's bottom).
- Measured, not guessed (macOS 27.0.1, Cmdr 0.59.0, 2026-11-04): `screencapture -l` of the running app's windows at 2x,
  the lights' edges found by a pixel scan: main and viewer (9, 17) draw at (9, 8), Settings (20, 29) at (20, 20). Every
  window's lights are the same size, so the only per-window difference is the inset: 14 px each, on a 23 px pitch (9 px
  gaps). AppKit also rendered a standard window's buttons offscreen at 2x in light and dark, key and inactive, hovered
  and not (a Swift script swizzling the key-appearance and mouse-in-group getters, then `cacheDisplay`); the fills,
  rims, glows, and glyph shapes and colors in `STYLE` are sampled from those renders.
- Verified against `pnpm build:demo` in headless Chromium at 1x and 2x (2026-11-04): main (9, 8), Settings (20, 20),
  Keyboard shortcuts (14, 9), each 14 px on a 23 px pitch, measured from the panel's outer edge (its hairline is an
  overlay, so it doesn't push the page or the lights in); Settings' own drag strip moves its panel.
- Light: a vertical fill that brightens toward the bottom, a dark rim strongest at the sides, a soft tinted glow below.
  Dark: a bright rim at the top and bottom and a bloom near the bottom, no glow. Inactive: light grey or dark grey with
  the matching rim. Hovering the group lights all three and shows every glyph, even on an inactive window, as macOS
  does.
- Glyphs: × and −; zoom shows full-screen arrows (inward ones while full screen, or while a panel is zoomed), as every
  Cmdr window is resizable and so full-screen capable, and Settings' real green light shows them.
- Active state: lit for the key window while the page has focus (`document.hasFocus()`, read a task after `focus` /
  `blur`, since focus moving into a panel's iframe blurs the top window), grey otherwise. The window host tracks the key
  window as it raises and closes panels.
- What each does. Main: zoom toggles the browser's full screen (inert where the page can't, as in a frame without
  `allow="fullscreen"`); close and minimize stay inert (`aria-disabled`), since a page can't close or hide its tab.
  Panel: close closes the window; zoom fills the desktop under the menu bar (capped at the window's `maxWidth`) and
  back; minimize stays inert, as there's no Dock to restore from. Labels reuse `ui.modalDialog.close`,
  `menu.window.minimize`, and `menu.window.zoom`. The lights aren't Tab stops and never take focus, as on macOS.
- The main lights sit under the menu bar: their `top` adds `--cmdr-demo-window-top`, which `menu-bar.svelte.ts` sets.

## Framed by another page

The landing page may embed the demo in an iframe. Nothing may assume a full browser window: sizes come from the frame's
own `window.innerWidth` / `innerHeight`, focus from `document.hasFocus()`, and the backend lookup on `window.parent` is
guarded (a cross-origin parent throws). Checked at 960×600 and framed by another origin (headless Chromium, 2026-11-04).
Testing an embed locally: Chrome's local-network checks block a public-looking page (a Playwright `route.fulfill`
origin) from framing `localhost`; serve the parent page from a real loopback server.

## Build and serve

- `pnpm build:demo` (in `apps/desktop`, or at the repo root, which runs this one): `pnpm -w build:listing-wasm` (the
  engine into `crates/cmdr-listing-wasm/pkg/`, gitignored), then `CMDR_DEMO_BUILD=1 vite build` with SvelteKit
  `paths.base = '/demo'`, output in `apps/desktop/build-demo/` (gitignored; eslint and oxfmt skip it). Never into
  `build/`: Tauri embeds that dir, and the adapter wipes its output dir on every build.
- `pnpm dev:demo`: the same engine build, then the dev server under `/demo/`. Pass `CMDR_VITE_PORT` for a port other
  than 1420. The first load after a config change can sit on the spinner while Vite re-optimizes dependencies; reload.
- `virtual:cmdr-listing-wasm` is an alias (demo builds only) to the engine's JS glue; `listing-wasm.d.ts` types the
  subset `listing.ts` uses, so the typecheck passes without the generated package. `server.fs.allow` gets the `pkg/`
  dir, since SvelteKit's allow list doesn't reach `crates/` and the glue fetches its `.wasm` from there.
- The static host needs an SPA fallback to `index.html` for paths under `/demo/`, as adapter-static's `fallback`
  assumes.
- Hosting on getcmdr.com, the demo's CSP, and the Playwright smoke (`apps/website/e2e/demo.spec.ts`: boots without a
  page error, lists the home folder): `apps/website/DETAILS.md` § Browser demo. The CSP blocks off-origin fetches,
  `eval` / `new Function`, and inline scripts added after the build, though the smoke's server sends no CSP.

## Tests

- One colocated `*.test.ts` per module. They pin what breaks the demo silently: the tree's parents and its 1,200 photos
  (the engine refuses a bad tree at boot), the browser-safe map (real command ids, the canonical combo vocabulary, no
  clash in an overlapping scope, no browser-reserved combo left), no TS answer for an engine command
  (`engine-commands.test.ts`), the backend's routing order (TS tables, then `handles()`, then `unanswered`), the
  webview's JSON wire and event targets, the window panels' open/close/raise/refuse/zoom, the traffic lights' actions
  and key-window state, the Cmdr gold seed, and the boot role pick. The lights' look is QA'd against AppKit's renders in
  a real browser (§ Traffic lights).
- vitest aliases `#lib/demo/boot.js` to `boot-absent.ts`, `$app/paths` to `apps/desktop/src/test-app-paths-shim.ts`, and
  `virtual:cmdr-listing-wasm` to `apps/desktop/src/test-listing-wasm-shim.ts`; `listing.test.ts` mocks the last with a
  fake engine. `test-setup.ts` stubs `@tauri-apps/api/event` app-wide, so `webview.test.ts` unmocks it to test the real
  one.
- The real `.wasm` and the iframes' pages loading are covered outside vitest: `desktop-rust-listing-wasm` runs the
  engine's smoke, `apps/website/e2e/demo.spec.ts` boots the built demo, and the headless QA above.

## Answering a command

1. Find what's unanswered: open the demo, then read `window.__cmdrDemo.unanswered` (command → call count) in devtools.
   The first unanswered call per command also logs `[demo] no answer for …` at debug level.
2. A listing command belongs to the engine (`crates/cmdr-listing-wasm`): ask there, don't answer it in TS.
3. Look up its return type in `bindings.ts` (the `__TAURI_INVOKE<...>` or `typedError<Ok, Err>`).
4. Void command (returns `()`): add its bindings name to `silentCommands` in `answers.ts`. `VoidCommand` rejects a
   command that returns a value. Unanswered voids work anyway (the fallback answers `null`); listing them only keeps the
   log quiet. Any OTHER unanswered command rejects with a string, as Tauri rejects a command it lacks: a `null` posing
   as an answer crashed the search dialog once. Which are void (`Promise<void>`, or a `Result<(), E>` whose success is
   `null`) is `virtual:cmdr-demo-void-commands`, which `apps/desktop/scripts/vite-demo-void-commands.ts` reads off
   `bindings.ts` at build time (registered in `vite.config.js` for the demo build only, and in `vitest.config.ts`).
5. Value command: add an entry to `appAnswers` (app state) or `volumeAnswers` (volumes, paths, and the tree), keyed by
   its bindings (camelCase) name. The answer is the `ok` value (`RawAnswer<K>`); a `typedError` command's error branch
   is `refuse('openPath', { type: 'unsupported' })`, typed against that command's error, which `invoke` rejects with as
   it does for a Rust `Err`. Opening a folder or package uses exactly that: the app's own refusal for a platform with no
   opener. Opening a demo FILE (Enter) is the backend's: it sends `open-file-viewer` to the main window, so the demo's
   default app is Cmdr's own viewer. Whether a path is a file is the live volume's answer (the engine's
   `stat_paths_kinds`), ❌ never the boot tree's `demoEntries`: a set built from the tree once refused every copy, move,
   and rename the visitor made (the viewer itself already read them, through the volume the write engine wrote to;
   `viewer_test.rs`'s copied-renamed-moved cell pins that half).
6. Commands outside the bindings (generic `<R: Runtime>` ones called through raw `invoke`, see `lib/ipc/DETAILS.md`) go
   in `rawAnswers`, keyed by their snake_case wire name, typed by hand against the wrapper's return type.
7. Plugin commands (`plugin:<name>|<command>`) go in `plugins.ts`, or in `webview.ts`'s `windowAnswer` when they're
   about the calling window.

Answer as a fresh, personal-use Cmdr on a Mac with nothing extra would: no index (folder sizes are real anyway, from the
engine's fold: `crates/cmdr-listing-wasm/DETAILS.md` § Folder sizes), no AI, no network hosts, no updater
(`getManagedPolicy` says updates are disabled), no tags or custom folder icons. Icons: macOS draws the live ones and the
demo can't ask it, so `getIcons` answers every id with the app's bundled generic folder or document
(`static/icons/default-*.png`), through `$app/paths`' `asset()` so they resolve under `/demo`.

- Gotcha: the unanswered fallback answers `null`, which is only right for a void command. A value command left
  unanswered gets a `null` the app's types say can't happen: `getRecentSearches` did, and the search dialog's render
  threw on it, so ⌘F flashed the dialog and its boundary closed it. Answer every value command the app asks for.
- Search: ⌘F is the WASM engine's (`crates/cmdr-listing-wasm/DETAILS.md` § Search), so no TS table answers a search
  command (`engine-commands.test.ts` pins it). TS answers only `cloudAiConsentStatus` (no consent), which with
  `getAiStatus` unavailable keeps the dialog's AI mode off.

## Writes

- Every write, queue, path, and undo command goes to the real write engine in the WASM module
  (`crates/cmdr-listing-wasm/DETAILS.md` § Writes), on the same in-memory volume the panes list, so a copy lands in the
  other pane through `directory-diff`, the queue window controls it, and the operation log rolls it back. Gone on
  reload, like the rest of the demo's state. Verified in headless Chromium (2026-11-04): F7, F5, a conflict answered
  Skip, Shift+F6, F6, F8, the queue's pause and resume, and a roll back (`apps/website/e2e/demo.spec.ts`).
- The volume is writable and has no Trash (`volumes.ts`: `mountIsReadOnly: false`, `supportsTrash: false`, as the
  engine's `Volume::has_trash` answers), so F8 takes the app's permanent-delete confirm, as on a network share.
- `listing.ts` expands `~` in every path argument (`path`, `paths`, `parentPath`, `sources`, `sourcePaths`, `destPath`,
  `destination`, `from`, `to`, `dir`), as the desktop's commands do for `root`: the dialogs name the home folder `~`.
- TS answers only what isn't the engine's: `estimateOperationCost` is `[]` (only an S3 transfer has a bill).
- There's no ⌘Z for an operation in the app either: undo is the operation log's Roll back (⌘⌥L).
- Not answered yet: multi-rename (`open_multi_rename`, so its dialog fails to open), tags (`toggle_tags`), and the new
  folder dialog's AI name suggestions (`get_ai_status`).

## Drag and drop

On desktop, `start_drag_paths` hands the files to AppKit, which owns the mouse from then on and reports what it does
through wry's drag-drop events (`tauri://drag-enter`, `-over`, `-drop`, `-leave`) plus the swizzle's `drag-modifiers`.
`drag-session.ts` is that OS session for a page: its answer to `start_drag_paths` follows the pointer that's still down
and sends the same events to the main window. So the app's drag controller
(`file-explorer/pane/drag-drop-controller.svelte.ts`) runs unchanged: the hit test, the cursor label, auto-scroll, the
self-drag identity, and the real transfer dialog on the in-browser write engine.

- Events: `drag-modifiers` then `tauri://drag-enter` on the first move (or the release), `-over` per move, `-drop` where
  it lets go. Positions are client px, which is what the controller reads Tauri's as. Modifiers come off the pointer and
  key events (Option copies, ⌘ or Shift moves, as on desktop) and go out on enter and on each change.
- Leaving the page sends `-leave`, coming back `-enter` again (the controller's re-entry fingerprint keeps it a
  self-drag). Letting go outside the page is a clean no-op: nothing out there takes the files. Escape cancels, kept from
  the app as AppKit keeps it; so does `pointercancel`, a window `blur`, and a button that was already up when the
  session began (a quick flick).
- While it runs, the page's mouse events are kept from the app (`mousedown` through `contextmenu`, plus the release's
  own `click` a task later), as an OS drag loop keeps them from the webview.
- Touch: the app arms a drag on a held touch and keeps the page from scrolling under it, generic code for any touch
  browser (`file-explorer/drag/DETAILS.md` § Touch drags); the session follows that pointer like a mouse.
- The drag image: the app writes a PNG for the OS session (`tempDir`, `join`, `plugin-fs` `writeFile` / `remove`).
  `plugins.ts` joins paths for real and takes the writes and drops them, since nothing reads them; `plugins.test.ts`
  fails if a second app module starts using `plugin-fs`, which would need its own answer. The swizzle's state
  (`prepare_self_drag_overlay`, `clear_self_drag_overlay`, `set_self_drag_resolved_op`) is silent.
- Files dragged in from the computer: § Files dropped in from the computer.
- Verified in headless Chromium against `pnpm build:demo` (2026-11-04): View > Show hidden files lists `.zshrc` and the
  check follows; View > Command palette opens it; Go > Home runs; View > Sort by > Size checks Size once the sort lands;
  Tab > New tab adds a tab and reads ⌃T; Cmdr > Settings opens its panel, File's items grey while it's in front, and Tab
  > Close tab closes it; the keyboard path from ⌃F2 reaches the palette; nothing lands in `unanswered`.

## Right-click menus

- **What**: the native popups the app opens (`show_file_context_menu`, the breadcrumb's, a tab's, the date header's Sort
  by, the function key bar's, the viewer's text, a queue row's, a servers hub host's), drawn as HTML macOS 27 menus
  (`context-menu.ts`) with the bar's look and keys. The app already cancels the browser's own menu
  (`#lib/context-menu-ownership.ts`), so a right-click reaches the same `show_*` command it does on desktop.
- **Decision**: the rows are the desktop's own, through WASM. `crates/cmdr-menu`'s `context` holds every popup's rows
  and what a pick asks for; `cmdr-listing-wasm`'s `context_menu.rs` answers each `show_*` command with them and stashes
  what the menu went up over, as `commands/menu.rs` stashes it in `MenuState` (`crates/cmdr-listing-wasm/DETAILS.md` §
  The JS API). **Why**: the same as the bar's; a TS copy drifts on the first new item, and a pick that sent anything but
  the native event would need its own handler in the app.
- **Routing**: the backend asks `menu.opensPopup(cmd)` before the menu-state pushes. Each webview passes its label and
  where its pointer last went down (`InvokeOrigin`, tracked in `webview.ts`), so the backend draws the menu in the TOP
  frame at the pointer, offset by the panel's iframe for a secondary window, or at the `anchor` the app names for a
  keyboard-opened menu (which then starts on its first enabled row). Top frame on purpose: a native popup isn't clipped
  to its window, and one renderer serves every window. The command answers `null` once the menu is up, as the native one
  returns while its menu tracks.
- **Picks**: `menu.contextClick(id)`, then `menu-click.ts` sends it: an event to `main`, `queue`, or the viewer the menu
  went up over, or the `add_favorite` command (the desktop calls it directly, for its add gate). A row the last popup
  didn't offer enabled does nothing, in the engine as in the page.
- **Left out**: every OS-filled group (Open with, Share, the tag colors, the provider's actions, Services): a browser
  has no OS to ask. The image-search group stays hidden (no media index).
- **Keys**: ↑/↓, Home/End, → into a submenu, ← or Esc out, Esc closes, Enter / Space pick, Tab closes; every key is kept
  from the app while the menu is up. The header line is a `presentation` row, never a stop, and names the menu for a
  screen reader (`aria-label`). A click outside, or the page losing focus, closes it.
- **Shared with the bar**: `menu-surfaces.ts` (an open menu and its submenus: drawing, placement, highlight, the walk),
  `menu-click.ts` (performing a pick), and `menu-bar-style.ts`'s `MENU_SURFACE_STYLE`, injected once per document.
- Verified in headless Chromium against `pnpm build:demo` (2026-11-04): right-clicking `Documents` draws the folder menu
  named by its header, with the live shortcuts (F5, ⌥⌘C) and no OS-filled group; ↓ lands on Selection, hovering it opens
  the submenu beside it, and Selection > Select all selects the whole pane; no page error.

## Traffic lights

On desktop the OS draws the close, minimize, and zoom buttons over each window's overlay title bar, at the
`trafficLightPosition` the app gives that window, and the app only leaves them room. In a browser nothing draws them, so
`traffic-lights.ts` does, from the demo's own DOM: the main window's as a fixed group at its top-left, each panel's
absolutely placed over its frame (`windows.ts`).

- Placement has one source, the app's own numbers: the main window's `trafficLightPosition` is read from
  `tauri.conf.json` (imported), and a panel's from the `create_webview_window` options its opener sends (Settings (20,
  29), Keyboard shortcuts (14, 18), viewer and queue (9, 17), as `LogicalPosition` serializes, `{ x, y }`).
  `lightsInset` turns one into the close button's corner: `x` as is, `y - 9` for the top (tao grows the title-bar
  container to the button's frame plus `y`, and AppKit pins the button to the container's bottom).
- Measured, not guessed (macOS 27.0.1, Cmdr 0.59.0, 2026-11-04): `screencapture -l` of the running app's windows at 2x,
  the lights' edges found by a pixel scan: main and viewer (9, 17) draw at (9, 8), Settings (20, 29) at (20, 20). Every
  window's lights are the same size, so the only per-window difference is the inset: 14 px each, on a 23 px pitch (9 px
  gaps). AppKit also rendered a standard window's buttons offscreen at 2x in light and dark, key and inactive, hovered
  and not (a Swift script swizzling the key-appearance and mouse-in-group getters, then `cacheDisplay`); the fills,
  rims, glows, and glyph shapes and colors in `STYLE` are sampled from those renders.
- Verified against `pnpm build:demo` in headless Chromium at 1x and 2x (2026-11-04): main (9, 8), Settings (20, 20),
  Keyboard shortcuts (14, 9), each 14 px on a 23 px pitch, measured from the panel's outer edge (its hairline is an
  overlay, so it doesn't push the page or the lights in); Settings' own drag strip moves its panel.
- Light: a vertical fill that brightens toward the bottom, a dark rim strongest at the sides, a soft tinted glow below.
  Dark: a bright rim at the top and bottom and a bloom near the bottom, no glow. Inactive: light grey or dark grey with
  the matching rim. Hovering the group lights all three and shows every glyph, even on an inactive window, as macOS
  does.
- Glyphs: × and −; zoom shows full-screen arrows (inward ones while full screen, or while a panel is zoomed), as every
  Cmdr window is resizable and so full-screen capable, and Settings' real green light shows them.
- Active state: lit for the key window while the page has focus (`document.hasFocus()`, read a task after `focus` /
  `blur`, since focus moving into a panel's iframe blurs the top window), grey otherwise. The window host tracks the key
  window as it raises and closes panels.
- What each does. Main: zoom toggles the browser's full screen (inert where the page can't, as in a frame without
  `allow="fullscreen"`); close and minimize stay inert (`aria-disabled`), since a page can't close or hide its tab.
  Panel: close closes the window; zoom fills the desktop under the menu bar (capped at the window's `maxWidth`) and
  back; minimize stays inert, as there's no Dock to restore from. Labels reuse `ui.modalDialog.close`,
  `menu.window.minimize`, and `menu.window.zoom`. The lights aren't Tab stops and never take focus, as on macOS.
- The main lights sit under the menu bar: their `top` adds `--cmdr-demo-window-top`, which `menu-bar.svelte.ts` sets.

## Framed by another page

The landing page may embed the demo in an iframe. Nothing may assume a full browser window: sizes come from the frame's
own `window.innerWidth` / `innerHeight`, focus from `document.hasFocus()`, and the backend lookup on `window.parent` is
guarded (a cross-origin parent throws). Checked at 960×600 and framed by another origin (headless Chromium, 2026-11-04).
Testing an embed locally: Chrome's local-network checks block a public-looking page (a Playwright `route.fulfill`
origin) from framing `localhost`; serve the parent page from a real loopback server.

## Build and serve

- `pnpm build:demo` (in `apps/desktop`, or at the repo root, which runs this one): `pnpm -w build:listing-wasm` (the
  engine into `crates/cmdr-listing-wasm/pkg/`, gitignored), then `CMDR_DEMO_BUILD=1 vite build` with SvelteKit
  `paths.base = '/demo'`, output in `apps/desktop/build-demo/` (gitignored; eslint and oxfmt skip it). Never into
  `build/`: Tauri embeds that dir, and the adapter wipes its output dir on every build.
- `pnpm dev:demo`: the same engine build, then the dev server under `/demo/`. Pass `CMDR_VITE_PORT` for a port other
  than 1420. The first load after a config change can sit on the spinner while Vite re-optimizes dependencies; reload.
- `virtual:cmdr-listing-wasm` is an alias (demo builds only) to the engine's JS glue; `listing-wasm.d.ts` types the
  subset `listing.ts` uses, so the typecheck passes without the generated package. `server.fs.allow` gets the `pkg/`
  dir, since SvelteKit's allow list doesn't reach `crates/` and the glue fetches its `.wasm` from there.
- The static host needs an SPA fallback to `index.html` for paths under `/demo/`, as adapter-static's `fallback`
  assumes.
- Hosting on getcmdr.com, the demo's CSP, and the Playwright smoke (`apps/website/e2e/demo.spec.ts`: boots without a
  page error, lists the home folder): `apps/website/DETAILS.md` § Browser demo. The CSP blocks off-origin fetches,
  `eval` / `new Function`, and inline scripts added after the build, though the smoke's server sends no CSP.

## Tests

- One colocated `*.test.ts` per module. They pin what breaks the demo silently: the tree's parents and its 1,200 photos
  (the engine refuses a bad tree at boot), the browser-safe map (real command ids, the canonical combo vocabulary, no
  clash in an overlapping scope, no browser-reserved combo left), no TS answer for an engine command
  (`engine-commands.test.ts`), the backend's routing order (TS tables, then `handles()`, then `unanswered`), the
  webview's JSON wire and event targets, the window panels' open/close/raise/refuse/zoom, the traffic lights' actions
  and key-window state, the Cmdr gold seed, and the boot role pick. The lights' look is QA'd against AppKit's renders in
  a real browser (§ Traffic lights).
- vitest aliases `#lib/demo/boot.js` to `boot-absent.ts`, `$app/paths` to `apps/desktop/src/test-app-paths-shim.ts`, and
  `virtual:cmdr-listing-wasm` to `apps/desktop/src/test-listing-wasm-shim.ts`; `listing.test.ts` mocks the last with a
  fake engine. `test-setup.ts` stubs `@tauri-apps/api/event` app-wide, so `webview.test.ts` unmocks it to test the real
  one.
- The real `.wasm` and the iframes' pages loading are covered outside vitest: `desktop-rust-listing-wasm` runs the
  engine's smoke, `apps/website/e2e/demo.spec.ts` boots the built demo, and the headless QA above.

## Answering a command

1. Find what's unanswered: open the demo, then read `window.__cmdrDemo.unanswered` (command → call count) in devtools.
   The first unanswered call per command also logs `[demo] no answer for …` at debug level.
2. A listing command belongs to the engine (`crates/cmdr-listing-wasm`): ask there, don't answer it in TS.
3. Look up its return type in `bindings.ts` (the `__TAURI_INVOKE<...>` or `typedError<Ok, Err>`).
4. Void command (returns `()`): add its bindings name to `silentCommands` in `answers.ts`. `VoidCommand` rejects a
   command that returns a value. Unanswered voids work anyway (the fallback answers `null`); listing them only keeps the
   log quiet. Any OTHER unanswered command rejects with a string, as Tauri rejects a command it lacks: a `null` posing
   as an answer crashed the search dialog once. Which are void (`Promise<void>`, or a `Result<(), E>` whose success is
   `null`) is `virtual:cmdr-demo-void-commands`, which `apps/desktop/scripts/vite-demo-void-commands.ts` reads off
   `bindings.ts` at build time (registered in `vite.config.js` for the demo build only, and in `vitest.config.ts`).
5. Value command: add an entry to `appAnswers` (app state) or `volumeAnswers` (volumes, paths, and the tree), keyed by
   its bindings (camelCase) name. The answer is the `ok` value (`RawAnswer<K>`); a `typedError` command's error branch
   is `refuse('openPath', { type: 'unsupported' })`, typed against that command's error, which `invoke` rejects with as
   it does for a Rust `Err`. Opening a folder or package uses exactly that: the app's own refusal for a platform with no
   opener. Opening a demo FILE (Enter) is the backend's: it sends `open-file-viewer` to the main window, so the demo's
   default app is Cmdr's own viewer. Whether a path is a file is the live volume's answer (the engine's
   `stat_paths_kinds`), ❌ never the boot tree's `demoEntries`: a set built from the tree once refused every copy, move,
   and rename the visitor made (the viewer itself already read them, through the volume the write engine wrote to;
   `viewer_test.rs`'s copied-renamed-moved cell pins that half).
6. Commands outside the bindings (generic `<R: Runtime>` ones called through raw `invoke`, see `lib/ipc/DETAILS.md`) go
   in `rawAnswers`, keyed by their snake_case wire name, typed by hand against the wrapper's return type.
7. Plugin commands (`plugin:<name>|<command>`) go in `plugins.ts`, or in `webview.ts`'s `windowAnswer` when they're
   about the calling window.

Answer as a fresh, personal-use Cmdr on a Mac with nothing extra would: no index (folder sizes are real anyway, from the
engine's fold: `crates/cmdr-listing-wasm/DETAILS.md` § Folder sizes), no AI, no network hosts, no updater
(`getManagedPolicy` says updates are disabled), no tags or custom folder icons. Icons: macOS draws the live ones and the
demo can't ask it, so `getIcons` answers every id with the app's bundled generic folder or document
(`static/icons/default-*.png`), through `$app/paths`' `asset()` so they resolve under `/demo`.

- Gotcha: the unanswered fallback answers `null`, which is only right for a void command. A value command left
  unanswered gets a `null` the app's types say can't happen: `getRecentSearches` did, and the search dialog's render
  threw on it, so ⌘F flashed the dialog and its boundary closed it. Answer every value command the app asks for.
- Search: ⌘F is the WASM engine's (`crates/cmdr-listing-wasm/DETAILS.md` § Search), so no TS table answers a search
  command (`engine-commands.test.ts` pins it). TS answers only `cloudAiConsentStatus` (no consent), which with
  `getAiStatus` unavailable keeps the dialog's AI mode off.

## Writes

- Every write, queue, path, and undo command goes to the real write engine in the WASM module
  (`crates/cmdr-listing-wasm/DETAILS.md` § Writes), on the same in-memory volume the panes list, so a copy lands in the
  other pane through `directory-diff`, the queue window controls it, and the operation log rolls it back. Gone on
  reload, like the rest of the demo's state. Verified in headless Chromium (2026-11-04): F7, F5, a conflict answered
  Skip, Shift+F6, F6, F8, the queue's pause and resume, and a roll back (`apps/website/e2e/demo.spec.ts`).
- The volume is writable and has no Trash (`volumes.ts`: `mountIsReadOnly: false`, `supportsTrash: false`, as the
  engine's `Volume::has_trash` answers), so F8 takes the app's permanent-delete confirm, as on a network share.
- `listing.ts` expands `~` in every path argument (`path`, `paths`, `parentPath`, `sources`, `sourcePaths`, `destPath`,
  `destination`, `from`, `to`, `dir`), as the desktop's commands do for `root`: the dialogs name the home folder `~`.
- TS answers only what isn't the engine's: `estimateOperationCost` is `[]` (only an S3 transfer has a bill).
- There's no ⌘Z for an operation in the app either: undo is the operation log's Roll back (⌘⌥L).
- Not answered yet: multi-rename (`open_multi_rename`, so its dialog fails to open), tags (`toggle_tags`), and the new
  folder dialog's AI name suggestions (`get_ai_status`).

## Drag and drop

On desktop, `start_drag_paths` hands the files to AppKit, which owns the mouse from then on and reports what it does
through wry's drag-drop events (`tauri://drag-enter`, `-over`, `-drop`, `-leave`) plus the swizzle's `drag-modifiers`.
`drag-session.ts` is that OS session for a page: its answer to `start_drag_paths` follows the pointer that's still down
and sends the same events to the main window. So the app's drag controller
(`file-explorer/pane/drag-drop-controller.svelte.ts`) runs unchanged: the hit test, the cursor label, auto-scroll, the
self-drag identity, and the real transfer dialog on the in-browser write engine.

- Events: `drag-modifiers` then `tauri://drag-enter` on the first move (or the release), `-over` per move, `-drop` where
  it lets go. Positions are client px, which is what the controller reads Tauri's as. Modifiers come off the pointer and
  key events (Option copies, ⌘ or Shift moves, as on desktop) and go out on enter and on each change.
- Leaving the page sends `-leave`, coming back `-enter` again (the controller's re-entry fingerprint keeps it a
  self-drag). Letting go outside the page is a clean no-op: nothing out there takes the files. Escape cancels, kept from
  the app as AppKit keeps it; so does `pointercancel`, a window `blur`, and a button that was already up when the
  session began (a quick flick).
- While it runs, the page's mouse events are kept from the app (`mousedown` through `contextmenu`, plus the release's
  own `click` a task later), as an OS drag loop keeps them from the webview.
- Touch: the app arms a drag on a held touch and keeps the page from scrolling under it, generic code for any touch
  browser (`file-explorer/drag/DETAILS.md` § Touch drags); the session follows that pointer like a mouse.
- The drag image: the app writes a PNG for the OS session (`tempDir`, `join`, `plugin-fs` `writeFile` / `remove`).
  `plugins.ts` joins paths for real and takes the writes and drops them, since nothing reads them; `plugins.test.ts`
  fails if a second app module starts using `plugin-fs`, which would need its own answer. The swizzle's state
  (`prepare_self_drag_overlay`, `clear_self_drag_overlay`, `set_self_drag_resolved_op`) is silent.
- Files dragged in from the OS do nothing: the window cancels `dragover` / `drop`, else the browser opens the file in
  place of the demo. Decision: no import. A browser hands over bytes with no path, and the in-memory volume takes no
  outside bytes today (the engine would need a create-file-from-bytes write and the controller a non-path drop source),
  which is a feature, not a drag shim.
- Verified in headless Chromium against `pnpm build:demo` (2026-11-04): a mouse drag of `notes.md` from `~` to the other
  pane's `~/Downloads` shows "Move to Downloads" and opens Move 1 file, which lands it; with Option it's Copy; dropping
  onto the Documents row moves it there; an iPad-emulated touch held 450 ms then dragged does the same, and a quick
  swipe starts no drag; `__cmdrDemo.unanswered` stays empty.

## Files dropped in from the computer

`os-drop.ts` answers a browser's HTML drag events for files from outside the page (#957). A browser hands over `File`
objects with no path, so the bytes land on the in-memory disk first, then the app hears wry's drop events with the
staged paths. The controller runs unchanged: hit test, transfer dialog, conflict handling, write engine. No `if (demo)`.

- While over the page: `drag-image-size` (large, so the app's overlay stays hidden, as for a Finder drag on desktop: the
  browser draws the OS's own drag image), `drag-modifiers`, `tauri://drag-enter` with `/.drops/incoming` (names aren't
  readable before the drop; a demo-disk path keeps the Move-or-Copy verdict the staged paths get), `-over` per
  `dragover`, `-leave` when the `dragenter`/`dragleave` count drops to zero. `dragover` and `drop` are always cancelled,
  so the browser never opens a dropped file or follows a dropped link in place of the demo.
- On `drop`: `readDrop` takes `webkitGetAsEntry()` entries at once (the `DataTransfer` empties when the handler
  returns), walks folders (folders before their contents, `readEntries` until an empty batch), and falls back to the
  flat `files`. `importDropped` sends the shape (no bytes) to the engine's `importPlan`, reads bytes only for the
  accepted files, lands each through `importFile`, then `tauri://drag-drop` carries the staged top-level paths.
- The engine side, the staging folder, and the 10 MB per-visit budget: `crates/cmdr-listing-wasm/DETAILS.md` § Files
  dropped in from the computer. A cut-short drop shows the `ui.demoDrop.overBudget` toast (from `backend.ts`), after the
  part that fit lands.
- **Decision/Why: Move, the app's standard.** The staged files sit on the demo disk, so the controller picks Move, as it
  does for a Finder drag from the Mac's own disk, and the transfer leaves no staged copy behind (Option still copies).
  Collisions go through the transfer dialog's conflict handling, as for any drop.
- Verified in headless Chromium against `pnpm build:demo` with a synthetic `DataTransfer` drop (2026-11-05): pinned by
  `apps/website/e2e/demo.spec.ts` (a drop lands through "Move 1 file", a second drop of the same name shows the clash,
  an 11 MB drop shows the toast and lands nothing).

## Links

- Every external link the app opens goes through `openExternalUrl` (`plugin:opener|open_url`): About, the feedback
  dialog, onboarding's "Star the repo on GitHub" (`GITHUB_REPO_URL` in `lib/beta-links.ts`), and Markdown links. The
  demo answers that one IPC (`opener.ts`, wired in `plugins.ts`) with a new browser tab, `noopener,noreferrer` so the
  opened page can't reach back into the demo. The backend lives in the top frame, so a link clicked in a panel opens
  from there too.
- Only `http:`, `https:`, and `mailto:`. Anything else (`file:`, `javascript:`, a System Settings URL, a string that
  isn't a URL) gets a plain-string refusal, as the plugin refuses a URL outside its scope, and opens nothing.
- Risk: the tab opens a few microtasks after the click (the invoke is a promise chain), which Chromium and Firefox count
  as the same user gesture; a stricter popup blocker may still hold it.

## The listing seam

- `listing.ts` (`startListingEngine`) imports the engine module, `await`s its `init()`, and builds one `DemoListing`
  over `demoEntries` (`{ volumeId, root, entries }`, the shape `crates/cmdr-listing-wasm/src/tree.rs` reads). Loading
  starts at boot, so the `.wasm` downloads while the app starts; the first listing command waits for it.
- Routing: the backend tries its TS tables first, then `DemoMenuBar.handles(cmd)`, then `DemoListing.handles(cmd)`, then
  `unanswered`. So a new engine command needs no TS change, and a TS answer for an engine command would shadow it.
- `demo.invoke(cmd, args)` takes the Tauri (snake_case) name and the camelCase IPC args, and resolves or rejects exactly
  as `invoke` does. `listing.ts` expands `~` in `path` / `paths` first (`demoPath`): the engine has no home folder.
- Engine events (`onEvent(name, payload)`, Tauri names) go to the backend's fan-out a task late, in order. Why the
  delay: the in-memory read can finish inside `list_directory_start_streaming`, but the pane expects that command's
  answer before the listing's first event, as Rust's spawned read guarantees.
- A failed engine load logs once (`[demo] the listing engine did not load`) and every listing command then lands in
  `unanswered`.
- The engine answers `path_exists`, `destination_exists`, and `stat_paths_kinds` from the live volume; `volumes.ts`
  keeps `getSyncStatus` (nothing syncs) and `storedSpellings` (the demo volume stores every path as given).
- Brief mode's column widths come from the engine too (`cmdr-brief-columns`): it answers `has_font_metrics`, the
  frontend measures and sends `store_font_metrics` / `extend_font_metrics` as on desktop, and
  `get_brief_column_text_widths` shrink-wraps each column. So the demo has no TS answer for any of the four. Verified in
  headless Chromium (2026-11-04): Documents' columns fit their names, `旅行の計画.txt` included, and the 1,200-photo
  folder lays out five fitted columns.

## Placeholders David decides

- `demo-tree.ts`: the home folder. Today a generic `/Users/you` with documents, downloads, a 1,200-photo folder to feel
  scrolling and sorting, dotfiles to try hidden files, and a fixed "now" (2026-10-01) so dates read the same for every
  visitor.
- `browser-shortcuts.ts`: the browser-safe shortcut map. The browser takes ⌘T, ⌘W, ⌘⇧T, ⌘N, and ⌘1–⌘9 before the page
  sees them, so those commands move to ⌃. Seeded into `shortcuts.json` as `shortcut:<commandId>` keys, the same delta
  the Settings editor writes, so every hint in menus, the palette, and the function bar shows the remapped combo with no
  demo awareness in the shortcut system.
- `plugins.ts` seeds `settings.json` as a user who just finished the current wizard (`onboarding.completed` and
  `onboarding.upgradeNudgeShown`, the two flags `startup-gates.ts::completeOnboardingWizard` writes): a visitor came to
  try the panes. The full wizard is no first impression here: step 1 is Full Disk Access, which a browser can't grant,
  and its Allow path needs a restart. It also seeds the app color on Cmdr gold. The registry default is `system`, and
  the desktop default stays so; the seed is the demo's only lever. Settings still offers "System theme color", which in
  the demo is the virtual Mac's accent, the blue `getAccentColor` answers. Since the seed differs from the registry
  default, App color shows its reset arrow, and reset picks that blue.
