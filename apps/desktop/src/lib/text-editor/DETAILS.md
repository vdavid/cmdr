# Text editor: details

Depth behind `CLAUDE.md`. What macOS counts as a text editor, the three launch forms, and how a "Choose an app…" pick is
stored live in `src-tauri/src/file_system/DETAILS.md` § Text editor.

## The stored choice

`behavior.textEditorApp` is one string, told apart structurally by Rust's `parse_choice`:

- **`system`**: the macOS plain-text default, launched with `open -t`, so a user who never touches the setting gets
  exactly what F4 always did.
- **A bundle id**: a listed editor, launched with `open -b`. It survives an update that moves or renames the bundle.
- **An absolute `.app` path**: a pick of a specific copy, launched with `open -a`.

`getTextEditorChoice` reads a missing, empty, or non-string value as `system`. That's also why a test whose settings
mock answers one number for every key still sees the system default. Linux runs `xdg-open` and ignores the choice.

## What a press can come back with

- **`opened`**: `open` accepted the request. Not proof a window appeared (a freshly downloaded VS Code can exit 0 with
  no window on its first launch). Nothing is said.
- **`chosen_app_missing_opened_default_instead`**: the chosen app isn't on this Mac right now, so Rust opened the file
  in the system default. The setting resets to `system` and the missing-app toast goes up. `openFileInEditor` still
  resolves `true`: an app was asked, so `file.edit` counts `editor_opened`.
- **`OpenInEditorFailure`** (`launchRefused`, `timedOut`): a plain error toast, the choice left alone, and `false`,
  which the pane maps to `launchFailed`. `timedOut` stays honest that the editor may still appear. A throw with no typed
  reason gets the `launchRefused` wording, so nothing escapes as a rejection.

## Why a removed app falls back instead of refusing

The user asked to edit this file, and the system default does that. The toast explains the surprise, and the reset keeps
the next press quiet. The toast names the fallback rather than the missing app: once a bundle is gone there's nothing
left to read a name from, and unlike the terminal there's no table to ask. `appMissing` says "can't find" rather than
"isn't installed", so it stays true for an app on a disk that's unmounted right now, which gets reset too; "Open
settings" leads straight back to the row.

## The toasts

Everything goes out under the one id `text-editor`, dismissed first and re-added. The dismiss makes the replacement
total: `addToast`'s same-id path keeps the first toast's `dismissal` and width, which are wrong for a different message.

- **Missing app**: persistent, 400 px wide, `TextEditorToastContent` with the message resolved at raise time and no
  Dismiss (the toast's own close button is enough).
- **`launchRefused`, `timedOut`**: a plain string toast at error level, transient.

"Open settings" calls `openSettingsToTextEditor()`, which opens **Behavior > Navigation & file ops** under the
`text-editor-toast` surface and anchors on `settingAnchorId('behavior.textEditorApp')`. No row renders that anchor yet,
so the section opens at its top.

## Testing

- `open-file-in-editor.test.ts` mocks the IPC, the settings store, and the toast surface, and pins what gets written and
  said: the choice sent, the reset, the dismiss-then-add order, and the wording per failure.
- `text-editor-toasts.a11y.test.ts` runs axe over the toast body with and without Dismiss.
- `apps/desktop/src/lib/file-explorer/pane/editor-open.test.ts` pins the guard and the press outcome, with this module
  mocked.
- `apps/desktop/src/routes/(main)/command-dispatch.characterization.test.ts` and
  `apps/desktop/src/lib/file-explorer/pane/search-pane-keys.test.ts` assert `openInEditor(path, 'system', false)` right
  after the key, synchronously, which is why `openFileInEditor` reads the choice before its first `await`. Their
  `openInEditor` mocks resolve an `opened` report, because this module reads it.
- No Playwright spec: the `playwright-e2e` build records only the file path into `open_mock`, so which app would have
  launched is a Rust unit test (`text_editor.rs`).
