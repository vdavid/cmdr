# Quit prompt

The main window's view of a held quit. The backend owns the decision and the clock
(`apps/desktop/src-tauri/src/quit/CLAUDE.md`); this only renders and answers.

## Module map

- `quit-prompt.svelte.ts`: the `quitPrompt` store (opens on `quit-requested`, mirrors the countdown, sends the answer)
  plus `initQuitPrompt` / `cleanupQuitPrompt`, wired in `routes/(main)/+layout.svelte`.
- `QuitConfirmationDialog.svelte`: prop-driven and inert. Takes the operations, the seconds, and two callbacks.

## Must-knows

- **The countdown here is decoration.** If this module never runs, Rust's timer still fires and the app still quits.
  ❌ Never make the frontend the authority.
- **It's derived from a wall-clock target, not decremented per tick**, so a throttled or busy webview shows the honest
  number instead of drifting behind the backend.
- **`initQuitPrompt()` runs synchronously at the top of `onMount`**, before the awaited setup. The gate can hold a quit
  at any moment, and a missed `quit-requested` means the app quits on its own countdown with no dialog ever shown.
- **The dialog is `topmost`** (`--z-modal-top`), so it's answerable over a modal conflict prompt or a progress dialog.
  That prop exists for this one dialog; ❌ don't spend it elsewhere (two topmost dialogs are back to racing on DOM
  order). Pinned by `QuitConfirmationDialog.svelte.test.ts`.
- **Escape and the × mean "keep working"**, the answer that loses nothing.
- **Confirming leaves the dialog up.** The app is about to disappear; hiding it first flashes the file panes on the way
  out.
- **"Keep working" is not a snooze** — the backend deletes the countdown. Keep any copy edit honest about that.

Deeper notes live with the backend gate: `apps/desktop/src-tauri/src/quit/DETAILS.md`.
