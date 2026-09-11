# Text editor (frontend)

Which app F4 opens a file in, and everything said around that launch. Backend counterpart:
`src-tauri/src/file_system/text_editor.rs`. Whether a row has a real file at all is the pane's call
(`file-explorer/pane/editor-open.ts`); the Settings row is
`apps/desktop/src/lib/settings/sections/TextEditorSelect.svelte`.

## Module map

- **`open-file-in-editor.ts`**: `openFileInEditor(path)`, the one entry point. Resolves whether an app was asked to open
  the file; never throws.
- **`editor-hint.ts`**: pure. `decideEditorHint` says whether a press shows the one-time hint and spends its flag.
- **`text-editor-setting.ts`**: reads and writes `behavior.textEditorApp` and `behavior.textEditorHintSeen`, plus the
  Settings deep link.
- **`text-editor-choice.ts`**: `SYSTEM_DEFAULT_EDITOR_CHOICE`. **`TextEditorToastContent.svelte`**: the one toast body
  (a resolved message, an optional Dismiss, "Open settings").

## Must-knows

- **The guard lives in the pane, ❌ never here.** Callers reach this module only through `openInEditorOrExplain`, so a
  phone's or an archive's row is refused before any launch, and the setting never widens what F4 accepts.
- **Rust decides, this side words it.** The stored choice is PASSED into `openInEditor` on every press (Rust's settings
  loader reads once, at startup), and Rust answers with a typed report. ❌ No sentence crosses IPC.
- **The hint flag is spent only when the hint shows**, or when a non-`system` choice proves the setting was found. ❗
  With no other editor installed, say nothing and leave it UNSPENT. `editor-hint.ts` owns the table.
- **Ask about other editors only while the hint is due** (flag unspent AND choice `system`); every other press is a plain
  launch.
- **A removed app still opens the file**, in the system default: `chosen_app_missing_opened_default_instead` resets the
  setting to `system` and raises a persistent toast. ❗ Word it from the report's `openedInName`, the app that DID get
  the file: once a bundle is gone, nothing is left to read its name from.
- **One toast id, `text-editor`, dismissed before every add**, ❌ never a one-slot `toastGroup`: a group full of
  persistent toasts drops the incoming one (`open-terminal/DETAILS.md` § The toasts).
- **Keep `text-editor-choice.ts` import-free**, so `settings/sections/` can import it without a cycle through
  `$lib/settings` (`import-cycles`).
- **Read both settings before the first `await`** in `openFileInEditor`: two dispatch tests assert the `openInEditor`
  call synchronously after the key press.

What each outcome does, the hint table, why a removed app falls back, the toasts, and testing: `DETAILS.md`.
