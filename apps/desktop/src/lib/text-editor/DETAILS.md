# Text editor: details

Depth behind `CLAUDE.md`. What macOS counts as a text editor, the three launch forms, and how a "Choose an app…" pick is
stored live in `src-tauri/src/file_system/DETAILS.md` § Text editor. The Settings row that picks the app is
`apps/desktop/src/lib/settings/sections/DETAILS.md` § "Edit files in".

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
  no window on its first launch). Nothing is said, unless the hint is due.
- **`chosen_app_missing_opened_default_instead`**: the chosen app isn't on this Mac right now, so Rust opened the file
  in the system default. The setting resets to `system` and the missing-app toast goes up. `openFileInEditor` still
  resolves `true`: an app was asked, so `file.edit` counts `editor_opened`.
- **`OpenInEditorFailure`** (`launchRefused`, `timedOut`): a plain error toast, the choice left alone, and `false`,
  which the pane maps to `launchFailed`. `timedOut` stays honest that the editor may still appear. A throw with no typed
  reason gets the `launchRefused` wording, so nothing escapes as a rejection.

## The hint

The one-time hint is the only place this setting is advertised. `openFileInEditor` reads the choice and
`behavior.textEditorHintSeen` before its first `await`, asks Rust about other editors (`askAboutOtherEditors`) only while
the flag is unspent AND the choice is `system`, and hands the report to the pure `decideEditorHint`:

- **Hint already spent**: nothing said, nothing written.
- **The launch threw** (`launchRefused`, `timedOut`): no hint, and the flag stays unspent.
- **The choice isn't `system`**: no hint, and the flag is spent silently. They found the setting already, and Rust
  wasn't asked.
- **`otherEditorsInstalled` is `null`** (Linux, or not asked): no hint, no write. This row is the guarantee the hint
  never fires on Linux, where `xdg-open` reports nothing about other editors.
- **`otherEditorsInstalled` is `false`**: no hint, and ❗ the flag stays UNSPENT, so someone who installs Sublime Text
  next month is still told.
- **`otherEditorsInstalled` is `true`**: the hint shows, naming `openedInName` (the unnamed wording when there's none),
  and the flag is spent.

**Decision / why spend on show**: the terminal's rule (`apps/desktop/src/lib/open-terminal/first-use-pick.ts`), for the
same reason: a flag spent on a Mac with nothing else to pick would eat the hint that matters later.

**Decision / why ask only while due**: after the hint is spent nothing reads the answer. Rust computes it after `open`
has spawned, from ids and installed-ness only, so it never delays the editor or turns an editor that did open into a
`timedOut`.

**Decision / no adoption of a running editor**: unlike terminals there's a real system default already, and "running
right now" says little about which editor someone wants for their files.

`getTextEditorHintSeen` reads anything but an explicit `false` as spent: a corrupt value can cost the user the hint, but
never bring back one that showed.

## Why a removed app falls back instead of refusing

The user asked to edit this file, and the system default does that. The toast explains the surprise, and the reset keeps
the next press quiet. The toast names the fallback rather than the missing app: once a bundle is gone there's nothing
left to read a name from, and unlike the terminal there's no table to ask. `appMissing` says "can't find" rather than
"isn't installed", so it stays true for an app on a disk that's unmounted right now, which gets reset too; "Open
settings" leads straight back to the row.

## The toasts

Everything goes out under the one id `text-editor`, dismissed first and re-added. The dismiss makes the replacement
total: `addToast`'s same-id path keeps the first toast's `dismissal` and width, which are wrong for a different message.
A missing-app outcome only happens for a non-`system` choice, so it and the hint never come from the same press.

- **The hint**: persistent, 400 px wide, `TextEditorToastContent` with Dismiss. Its copy names no Settings path, because
  "Open settings" goes straight to the row.
- **Missing app**: persistent, 400 px wide, `TextEditorToastContent` with the message resolved at raise time and no
  Dismiss (the toast's own close button is enough).
- **`launchRefused`, `timedOut`**: a plain string toast at error level, transient.

"Open settings" calls `openSettingsToTextEditor()`, which opens **Behavior > Navigation & file ops** under the
`text-editor-toast` surface and scrolls to `settingAnchorId('behavior.textEditorApp')`, the anchor the row's
`SettingRow` stamps. An already-open Settings window keeps its search query, so a leftover query can filter the card
away and the scroll does nothing; every deep link shares that.

## Testing

- `editor-hint.test.ts`: one test per hint rule. Pure.
- `open-file-in-editor.test.ts` mocks the IPC, the settings store, and the toast surface, and pins what gets written and
  said: the choice sent, when other editors are asked about, the hint and which outcomes spend its flag, the reset, the
  dismiss-then-add order, and the wording per failure. Its top-level `beforeEach` spends the hint, so the launch and
  failure suites stay about their own subject.
- `text-editor-setting.test.ts`: the choice and flag readers, and the deep link.
- `text-editor-toasts.a11y.test.ts` runs axe over the toast body with and without Dismiss.
- `apps/desktop/src/lib/file-explorer/pane/editor-open.test.ts` pins the guard and the press outcome, with this module
  mocked.
- `apps/desktop/src/routes/(main)/command-dispatch.characterization.test.ts` and
  `apps/desktop/src/lib/file-explorer/pane/search-pane-keys.test.ts` assert `openInEditor(path, 'system', false)` right
  after the key, synchronously, which is why `openFileInEditor` reads its settings before its first `await`. The first
  mocks every setting as `100`, which reads as a spent hint; the second mocks the hint spent. Both `openInEditor` mocks
  resolve an `opened` report, because this module reads it.
- No Playwright spec of its own: the `playwright-e2e` build records only the file path into `open_mock`, so which app
  would have launched is a Rust unit test (`text_editor.rs`). The "New file round-trip" test in
  `apps/desktop/test/e2e-playwright/file-operations.spec.ts` spends `behavior.textEditorHintSeen` before its ⇧F4:
  whether the hint fires depends on the editors on the machine running the suite, and the global `afterEach` fails a
  test that leaves a toast up.
