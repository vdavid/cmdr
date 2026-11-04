# Browser demo (`lib/demo/`)

The real Cmdr frontend running in a plain browser on a fake backend, for getcmdr.com/demo (issue #302). Every `invoke`
lands in a typed answer here; the listing commands go to the real listing engine, built to WASM
(`crates/cmdr-listing-wasm/CLAUDE.md`).

- `boot.ts`: picks the frame's role. `backend.ts`: the page's one backend (answers, stores, engine, event fan-out).
  `webview.ts`: one frame's `__TAURI_INTERNALS__`, forwarding to the backend and holding its listeners. `windows.ts`:
  secondary windows as in-page panels (Settings, Keyboard shortcuts, and the operation queue; the rest refused).
  `menu-bar.ts`: the macOS menu bar, drawn from `menu-engine.ts` (the desktop's own bar and fold, through WASM).
  `traffic-lights.ts`: the macOS window buttons the page draws, for the main window and each panel.
- `answers.ts` (app commands), `volumes.ts`, `plugins.ts` (in-memory `plugin:store`, path, app), `listing.ts` (routes to
  the WASM engine, loaded once by `engine-module.ts`), `unanswered.ts` (`window.__cmdrDemo.unanswered`; an unanswered
  void resolves, any other rejects, DETAILS § Answering a command).
- `demo-tree.ts` and `browser-shortcuts.ts`: PLACEHOLDER content David still decides.

Run: `pnpm dev:demo` or `pnpm build:demo` (into `apps/desktop/build-demo/`, base `/demo`) in `apps/desktop`. Both build
the WASM engine first.

## Must-knows

- **Isolation is a hard rule.** The app-side footprint is the `hooks.client.ts` import, the `vite.config.js` define,
  aliases, and demo-only void-commands plugin, and the `app.d.ts` global. No `if (demo)` in app code: demo behavior goes
  through existing seams (an IPC answer, a store seed, a typed refusal). `cmdr/demo-isolation` fails any
  `__CMDR_DEMO_BUILD__` or `#lib/demo` outside.
- **No demo code in any other build.** Outside `CMDR_DEMO_BUILD=1`, `vite.config.js` aliases `#lib/demo/boot.js` to the
  empty `boot-absent.ts`. Verify with a normal `pnpm build` and a grep of `build/` for `__cmdrDemo`.
- **One backend per page, in the top frame; one webview per frame.** A secondary window is a same-origin iframe of its
  route, named after its window label, attached through `window.parent.__cmdrDemoBackend`. Events fan out to every
  attached webview. Keep per-window facts (size, focus) in `webview.ts`, app state in the backend.
- **The demo may itself be framed** (a landing-page embed): assume no full browser window, and never touch
  `window.parent` / `window.top` beyond the guarded backend lookup.
- **Answers are typed against `bindings.ts`** (`AnswerTable`, `VoidCommand`), so a drifted answer fails `svelte-check`.
  An error branch is `refuse(command, typedError)`, never a thrown `Error`.
- **No menu logic in TS.** Rows, words, shortcuts, enabled and checked state, and what a click sends all come from
  `cmdr-menu` through `DemoMenuBar`; the app's menu pushes reach it because no TS table answers them. A click sends the
  native item's own event, so the app's handlers are the ones that run.
- **No listing or write logic in TS.** The backend tries its TS tables first, then whatever `DemoListing.handles()`
  claims, so a TS answer for an engine command would silently shadow the real engine. `engine-commands.test.ts` pins the
  write, queue, path, and undo commands as engine-only.
- **Writes go through the real write engine, in memory, gone on reload** (`crates/cmdr-listing-wasm`'s
  `operations_host/`): copy, move, rename, new folder, delete, the queue, and the operation log's roll back. No Trash on
  the volume, so F8 takes the app's permanent-delete confirm. ❌ Never a TS success for a write.
- **No top-level `await`** in the boot path: WebKit can run an importer early, and Safari visitors run the demo.

Adding an answer, the frame model, the seam, the placeholders, and decisions: `DETAILS.md`.
