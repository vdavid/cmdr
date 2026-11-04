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
every window. The demo keeps that shape so secondary windows can come later without a rewrite.

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
- Opening a secondary window is not wired yet: `plugin:webview|create_webview_window` and friends land in `unanswered`.
  The intended shape: the backend's answer adds a same-origin `<iframe name="<label>" src="<base><route>">` in the top
  frame, styled as a window, and closing it removes the iframe (its `pagehide` detaches the webview). Verified by hand:
  an iframe of `/demo/settings` named `settings` renders Settings, and changing "Size colors" there recolors the main
  window's sizes through the fan-out (headless Chromium, 2026-11-04).
- ❗ The production CSP (`apps/website/nginx-demo-security-headers.conf`) says `frame-src 'none'`, which blocks exactly
  that iframe (seen under that CSP, 2026-11-04). Wiring windows means `frame-src 'self'` there.

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

## Answering a command

1. Find what's unanswered: open the demo, then read `window.__cmdrDemo.unanswered` (command → call count) in devtools.
   The first unanswered call per command also logs `[demo] no answer for …` at debug level.
2. A listing command belongs to the engine (`crates/cmdr-listing-wasm`): ask there, don't answer it in TS.
3. Look up its return type in `bindings.ts` (the `__TAURI_INVOKE<...>` or `typedError<Ok, Err>`).
4. Void command (returns `()`): add its bindings name to `silentCommands` in `answers.ts`. `VoidCommand` rejects a
   command that returns a value. Unanswered voids work anyway (the fallback answers `null`); listing them only keeps the
   log quiet.
5. Value command: add an entry to `appAnswers` (app state) or `volumeAnswers` (volumes, paths, and the tree), keyed by
   its bindings (camelCase) name. The answer is the `ok` value (`RawAnswer<K>`); a `typedError` command's error branch
   is `refuse('openPath', { type: 'unsupported' })`, typed against that command's error, which `invoke` rejects with as
   it does for a Rust `Err`. Opening a file uses exactly that: the app's own refusal for a platform with no opener.
6. Commands outside the bindings (generic `<R: Runtime>` ones called through raw `invoke`, see `lib/ipc/DETAILS.md`) go
   in `rawAnswers`, keyed by their snake_case wire name, typed by hand against the wrapper's return type.
7. Plugin commands (`plugin:<name>|<command>`) go in `plugins.ts`, or in `webview.ts`'s `windowAnswer` when they're
   about the calling window.

Answer as a fresh, personal-use Cmdr on a Mac with nothing extra would: no index, no AI, no network hosts, no updater
(`getManagedPolicy` says updates are disabled), no tags or custom folder icons. Icons: macOS draws the live ones and the
demo can't ask it, so `getIcons` answers every id with the app's bundled generic folder or document
(`static/icons/default-*.png`), through `$app/paths`' `asset()` so they resolve under `/demo`.

## Writes

- Decision: the demo volume is `mountIsReadOnly: true` (`volumes.ts`), so the app's own read-only handling answers every
  write a visitor tries: F7 and F8 get "This is a read-only volume", F5 "… is read-only. You can copy files from it, but
  not to it", and the volume chooser shows the lock (headless Chromium, 2026-11-04). Why: it's a seam the app already
  has and words well, so the demo needs no copy of its own and no app change.
- `writes.ts` is the backstop for a write that skips those gates (a drop, a clipboard paste, an agent): the typed
  refusal a read-only disk gives (`parentNotWritable` for single-item mutations, `read_only_device` for transfers).
  Never a pretend success, which would leave the pane showing a file that isn't there.
- Not refused yet: multi-rename (`open_multi_rename` is unanswered, so its dialog fails to open) and tags (`toggle_tags`
  has no error arm). Real file operations in the demo are #929.

## The listing seam

- `listing.ts` (`startListingEngine`) imports the engine module, `await`s its `init()`, and builds one `DemoListing`
  over `demoEntries` (`{ volumeId, root, entries }`, the shape `crates/cmdr-listing-wasm/src/tree.rs` reads). Loading
  starts at boot, so the `.wasm` downloads while the app starts; the first listing command waits for it.
- Routing: the backend tries its TS tables first, then `DemoListing.handles(cmd)`, then `unanswered`. So a new engine
  command (file operations are coming) needs no TS change, and a TS answer for an engine command would shadow it.
- `demo.invoke(cmd, args)` takes the Tauri (snake_case) name and the camelCase IPC args, and resolves or rejects exactly
  as `invoke` does. `listing.ts` expands `~` in `path` / `paths` first (`demoPath`): the engine has no home folder.
- Engine events (`onEvent(name, payload)`, Tauri names) go to the backend's fan-out a task late, in order. Why the
  delay: the in-memory read can finish inside `list_directory_start_streaming`, but the pane expects that command's
  answer before the listing's first event, as Rust's spawned read guarantees.
- A failed engine load logs once (`[demo] the listing engine did not load`) and every listing command then lands in
  `unanswered`.
- `volumes.ts` answers `pathExists` and `statPathsKinds` from the same tree; `getSyncStatus` (nothing syncs) and
  `storedSpellings` (the demo volume stores every path as given) live there too.
- Known gap: Brief mode's `get_brief_column_text_widths` isn't in the engine yet, so Brief columns stay at their
  provisional width (the pane logs a warning and carries on). It lives in the app crate today
  (`src-tauri/src/font_metrics/`); once it moves into a Tauri-free crate the engine answers it, `has_font_metrics` flips
  to `false` so the frontend sends `store_font_metrics` / `extend_font_metrics`, and no other TS changes.

## Placeholders David decides

- `demo-tree.ts`: the home folder. Today a generic `/Users/you` with documents, downloads, a 1,200-photo folder to feel
  scrolling and sorting, dotfiles to try hidden files, and a fixed "now" (2026-10-01) so dates read the same for every
  visitor.
- `browser-shortcuts.ts`: the browser-safe shortcut map. The browser takes ⌘T, ⌘W, ⌘⇧T, ⌘N, and ⌘1–⌘9 before the page
  sees them, so those commands move to ⌃. Seeded into `shortcuts.json` as `shortcut:<commandId>` keys, the same delta
  the Settings editor writes, so every hint in menus, the palette, and the function bar shows the remapped combo with no
  demo awareness in the shortcut system.
- `plugins.ts` seeds `settings.json` with onboarding done: a visitor came to try the panes.
