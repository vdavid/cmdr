# `fs:allow-temp-write` and `fs:allow-remove` granted without scope to main and debug windows

**Severity:** high
**Lens:** D — IPC boundary
**Confidence:** high

## Location

- `apps/desktop/src-tauri/capabilities/default.json` lines 5-39 (windows `main`, `debug`; permissions
  `fs:allow-temp-write`, `fs:allow-remove`)
- Sole frontend consumers (writeFile + remove on temp paths only):
  - `apps/desktop/src/lib/file-explorer/drag/drag-drop.ts:204,205,215,216,236,237,246,247`

## What

The default capability grants `fs:allow-temp-write` and `fs:allow-remove` to the `main` and `debug` windows with **no
`scope`/`allow` object** restricting which paths the FS plugin can target. Tauri's `tauri-plugin-fs` interprets a
permission without a scope as "the plugin command is callable; the path argument is whatever the caller passes." That
means the frontend (or any compromised content rendered inside the main webview) can call
`@tauri-apps/plugin-fs.writeFile(anyPath, …)` and `remove(anyPath)` for any path the OS-level perms allow — far beyond
the two temp files (`TEMP_ICON_FILENAME`, `TEMP_DRAG_IMAGE_FILENAME`) that the code actually needs.

The companion `opener:allow-open-path` permission for the same windows uses globs (`**/*` and `**/.*`) so opening files
follows the FS shape Cmdr wants. The two FS-plugin perms don't.

## Why it matters

1. **Data-loss blast radius.** `remove` with no scope can delete any file the user can delete: settings, the dev-mode
   data dir, the user's home — anything the process has write access to. The FE only needs to remove two well-known
   filenames under `tempDir()`.
2. **Defense-in-depth gap if the webview is ever compromised.** A future feature that renders untrusted markdown,
   embeds a third-party preview, or opens an iframe would inherit a backdoor capable of overwriting and deleting
   arbitrary user files via raw `invoke('plugin:fs|write_file', …)` — bypassing every typed-IPC and Tauri-command
   guardrail the rest of the audit cares about.
3. **Inconsistent with the documented split.** `capabilities/CLAUDE.md` § "One capability file per window type" says
   the main window's privilege budget is "filesystem access, drag-and-drop, clipboard, and the updater". This perm is
   broader than needed for that role, and the debug window inherits it for free via being listed in
   `default.json::windows`.

## Evidence

`default.json` lines 34-35:

```json
"fs:allow-temp-write",
"fs:allow-remove",
```

Tauri permission docs (`tauri-plugin-fs`): each FS perm accepts a `scope` array of glob/path patterns. Without one the
permission is unconstrained.

Actual consumers (only paths used are under the OS temp dir):

```ts
// drag-drop.ts
const tempPath = await tempDir()
const iconPath = await join(tempPath, TEMP_ICON_FILENAME)
const { writeFile } = await import('@tauri-apps/plugin-fs')
await writeFile(iconPath, bytes)
```

…and an analogous `remove(iconPath)` plus `writeFile(imagePath, …)` / `remove(imagePath)` for the drag image.

## Suggested fix

Replace each permission entry with the scoped object form so the plugin enforces the temp-dir constraint:

```json
{
    "identifier": "fs:allow-temp-write",
    "allow": [
        { "path": "$TEMP/cmdr-drag-icon.png" },
        { "path": "$TEMP/cmdr-drag-image.png" }
    ]
},
{
    "identifier": "fs:allow-remove",
    "allow": [
        { "path": "$TEMP/cmdr-drag-icon.png" },
        { "path": "$TEMP/cmdr-drag-image.png" }
    ]
}
```

(Substitute the actual constants from `drag-drop.ts`; `$TEMP` is Tauri's standard path variable for the OS temp dir.)
If the drag-image filename ever becomes dynamic, switch to `{ "path": "$TEMP/cmdr-drag-*.png" }`.

Optional: drop the FS-plugin perm from `debug.json`'s window list (currently inherited via `default.json::windows`
including `"debug"`) since the debug page has no drag-out feature.

## Notes

Pairs with the related `default-capability-shared-between-main-and-debug` finding: even after scoping, the debug
window still inherits the main window's permission set wholesale. Worth fixing both together.
