# Browser demo: details

## What it is

A playable demo on getcmdr.com/demo: the real Svelte frontend on a mocked backend, so a visitor tries the keyboard flow
without installing anything. Issue #302 holds the research and David's decisions (option (b): the real listing engine in
WASM; lives at `/demo`; phones get the same page; placeholders for the file tree and shortcut map until David reworks
them).

Every IPC call goes through `invoke()` from `@tauri-apps/api/core`, which talks to `window.__TAURI_INTERNALS__`.
`boot.ts` replaces that object with Tauri's own `mockIPC` (with `shouldMockEvents`, so `listen` / `emit` work in-page)
and `mockWindows('main')`, and nothing in the app knows it runs in a browser.

## Boot order

- `hooks.client.ts` imports `#lib/demo/boot.js` as its FIRST import, so ESM evaluates it before any other app module.
  `invoke` reads the internals at call time, so the mock only has to exist before the first call.
- `boot.ts` installs itself at module evaluation behind `if (__CMDR_DEMO_BUILD__)`. The guard keeps vitest (which
  defines the flag `false` and doesn't apply the alias) from installing the mock.
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

## Build and serve

- `pnpm build:demo` (`CMDR_DEMO_BUILD=1 vite build`): SvelteKit `paths.base = '/demo'`, output in
  `apps/desktop/build-demo/` (gitignored; eslint and oxfmt skip it). Never into `build/`: Tauri embeds that dir, and the
  adapter wipes its output dir on every build.
- `pnpm dev:demo`: the dev server, also under `/demo/`. Pass `CMDR_VITE_PORT` for a port other than 1420.
- The static host needs an SPA fallback to `index.html` for paths under `/demo/`, as adapter-static's `fallback`
  assumes.
- What's open: hosting it on the website (copying `build-demo/` into the website's deploy) and a Playwright smoke test
  that boots the demo build in CI, so an IPC change can't silently break it.

## Answering a command

1. Find what's unanswered: open the demo, then read `window.__cmdrDemo.unanswered` (command → call count) in devtools.
   The first unanswered call per command also logs `[demo] no answer for …` at debug level.
2. Look up its return type in `bindings.ts` (the `__TAURI_INVOKE<...>` or `typedError<Ok, Err>`).
3. Void command (returns `()`): add its bindings name to `silentCommands` in `answers.ts`. `VoidCommand` rejects a
   command that returns a value. Unanswered voids work anyway (the fallback answers `null`); listing them only keeps the
   log quiet.
4. Value command: add an entry to the right table, keyed by its bindings (camelCase) name. `appAnswers` for app state,
   `volumeAnswers` for volumes and paths, `listingSideAnswers` for listing-adjacent commands with nothing to compute.
   The answer is the `ok` value (`RawAnswer<K>`); a `typedError` command's error branch is a throw of the typed error
   value.
5. Commands outside the bindings (generic `<R: Runtime>` ones called through raw `invoke`, see `lib/ipc/DETAILS.md`) go
   in `rawAnswers`, keyed by their snake_case wire name, typed by hand against the wrapper's return type.
6. Plugin commands (`plugin:<name>|<command>`) go in `plugins.ts`.

Answer as a fresh, personal-use Cmdr on a Mac with nothing extra would: no index, no AI, no network hosts, no updater
(`getManagedPolicy` says updates are disabled), and no file icons (macOS draws them, and the demo can't ship Apple's
artwork).

## The listing seam

- `listing.ts` routes every listing command (`listingCommands`) to a `DemoListingEngine`, whose one method is
  `call(command, payload)`: the bindings name plus the IPC payload, the same camelCase object the Rust command
  deserializes. It returns the `ok` value or throws the typed error.
- The engine sends events through the `emit` it gets at start (`DemoListingEngineInit`), typed as `DemoListingEvent`
  against the bindings' payloads. `listing.ts` forwards each through `events.<name>.emit` in a fresh task, in order. Why
  the delay: the in-memory engine finishes a whole listing inside `listDirectoryStartStreaming`, but the pane expects
  that command's answer before the listing's first event, as Rust's spawned read guarantees.
- `loadEngine` is THE seam. Until the `wasm-bindgen` package lands it resolves `null`, and every listing command shows
  up as unanswered (the panes show a read failure). With it: a dynamic `import()` of the package (the WASM downloads
  while the app boots), its `init()`, and an adapter from its API to `DemoListingEngine` if the two differ.
- The engine starts from `demoEntries` (`DemoEntry`: path, kind, size, dates, symlink target; parent folders listed
  explicitly). If the WASM side wants another shape, adapt in `loadEngine`, not in the tree.
- `volumes.ts` answers `pathExists` from the same tree; `getSyncStatus` (nothing syncs) and `storedSpellings` (the demo
  volume stores every path as given) live in `listingSideAnswers`.

## Placeholders David decides

- `demo-tree.ts`: the home folder. Today a generic `/Users/you` with documents, downloads, a 1,200-photo folder to feel
  scrolling and sorting, dotfiles to try hidden files, and a fixed "now" (2026-10-01) so dates read the same for every
  visitor.
- `browser-shortcuts.ts`: the browser-safe shortcut map. The browser takes ⌘T, ⌘W, ⌘⇧T, ⌘N, and ⌘1–⌘9 before the page
  sees them, so those commands move to ⌃. Seeded into `shortcuts.json` as user customizations (the same delta the
  Settings editor writes), so every hint in menus, the palette, and the function bar shows the remapped combo with no
  demo awareness in the shortcut system.
- `plugins.ts` seeds `settings.json` with onboarding done: a visitor came to try the panes.
