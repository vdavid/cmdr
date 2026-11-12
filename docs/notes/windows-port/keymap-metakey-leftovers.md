# `metaKey` reads left after the off-macOS input fixes

The `rg -n "metaKey" apps/desktop/src -g '!*.test.ts'` sweep of task 26's input-bug work (#1102), classified at the
commit that adds this file. Every other hit is a "no modifier held" guard (`!e.metaKey && !e.ctrlKey && …`, a bare-key
handler bailing on any modifier), macOS-only code (Quick Look's panel-forwarded keys), or the matcher's own vocabulary
(`key-capture.ts`, `primary-modifier.ts`), and is fine as it is. These read `metaKey` (alone, or `metaKey || ctrlKey` as
"either") to mean the platform's primary modifier, outside the file list, the main window's keydown resolver, and the
viewer's copy, select all, and search, which now go through `holdsPrimaryModifier` (`lib/shortcuts/primary-modifier.ts`)
or a per-platform table.

Already planned in the keymap work (the search dialog's and the viewer's fixed keys join the registry):

- `lib/query-ui/query-shortcuts.ts:26,93,98,107`: `matchKey`'s `meta` mode, `routeEnterCombination`, and
  `routeModeNumberShortcut` read `metaKey` alone, so off macOS the dialog's `⌘N` / `⌘H` / `⌘1`–`⌘3` answer Super and
  `Ctrl+Enter` falls through to bare Enter's action.
- `lib/search/test-search-dialog-harness.ts:287`: the harness builds its events with `metaKey`.
- `routes/viewer/viewer-keyboard.ts:176`: `handleSearchToggleKey` takes `metaKey || ctrlKey` with Alt (`⌃⌥R` on macOS,
  `Super+Alt+R` off it).
- `routes/viewer/ViewerStatusBar.svelte:64`: the search chip's fixed chord per OS.

For the lead to file:

- `lib/error-reporter/ErrorReportDialog.svelte:258` and `lib/feedback/FeedbackDialog.svelte:130`: send on
  `metaKey || ctrlKey` plus Enter, so `⌃Enter` sends on macOS and `Super+Enter` on Linux.
- `lib/settings/components/SettingsSidebar.svelte:229`: the sidebar search's select all on `metaKey || ctrlKey` plus A.
- The browser demo (`lib/demo/`), whose drawn menu bar is always the macOS one: `os-drop.ts:174` and
  `drag-session.ts:43` read `metaKey` as the copy-drag modifier for every visitor (Ctrl on a Windows or Linux visitor's
  real file manager); `context-menu.ts:91` and `menu-bar.svelte.ts:217` skip a popup accelerator on `metaKey`; the
  `onlyControl` checks in `menu-bar.svelte.ts:233` and `windows.ts:293` read Ctrl alone as macOS's ⌃. They follow the
  demo's bar, which follows the visitor's OS only once that's decided.
