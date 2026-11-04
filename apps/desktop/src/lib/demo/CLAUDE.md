# Browser demo (`lib/demo/`)

The real Cmdr frontend running in a plain browser on a fake backend, for getcmdr.com/demo (issue #302). Every `invoke`
lands in a typed answer here; the listing commands go to the real listing engine, built to WASM.

- `boot.ts`: installs `mockIPC` + `mockWindows`, routes each command. `hooks.client.ts` imports it first.
- `answers.ts` (app commands), `volumes.ts`, `plugins.ts` (in-memory `plugin:store`, path, window, app), `listing.ts`
  (the WASM seam), `unanswered.ts` (`window.__cmdrDemo.unanswered`).
- `demo-tree.ts` and `browser-shortcuts.ts`: PLACEHOLDER content David still decides.

Run: `pnpm dev:demo` or `pnpm build:demo` (into `apps/desktop/build-demo/`, base `/demo`) in `apps/desktop`.

## Must-knows

- **No demo code in any other build.** Outside `CMDR_DEMO_BUILD=1`, `vite.config.js` aliases `#lib/demo/boot.js` to the
  empty `boot-absent.ts`. Import demo modules only from `boot.ts`; anything else bypasses the alias and ships to users.
  Verify with a normal `pnpm build` and a grep of `build/` for `__cmdrDemo`.
- **Answers are typed against `bindings.ts`** (`AnswerTable`, `VoidCommand` in `answer-types.ts`), so a drifted answer
  fails `svelte-check`. Raw-`invoke` commands (`rawAnswers`) are typed by hand against their wrapper.
- **An error branch is a throw** of the typed error value (never an `Error`), as a Rust `Err` reaches `invoke`.
- **No listing logic in TS.** Sorting, hidden filtering, fuzzy jump, and the rest belong to the WASM engine;
  `listing.ts` only routes, so the demo lists exactly like the app.
- **Engine events go out a task late**, after the command that caused them answers, as Rust's spawned read does.
- **No top-level `await`** in the boot path: WebKit can run an importer early, and Safari visitors run the demo.

Adding an answer, the seam, the placeholders, and decisions: `DETAILS.md`.
