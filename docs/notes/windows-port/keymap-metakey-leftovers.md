# `metaKey` reads left after the off-macOS input fixes

The `rg -n "metaKey" apps/desktop/src -g '!*.test.ts'` sweep of task 26 (#1102), re-checked after the keymap work's last
change. Every other hit is a "no modifier held" guard (`!e.metaKey && !e.ctrlKey && …`, a bare-key handler bailing on
any modifier), macOS-only code (Quick Look's panel-forwarded keys, the viewer's ⌘-arrow document edges), or the
matcher's own vocabulary (`key-capture.ts`, `primary-modifier.ts`), and is fine as it is. The file list, the main
window's keydown resolver, the search dialog, the viewer, the error report and feedback dialogs' send key, and the
Settings sidebar search's select all go through `holdsPrimaryModifier` / `holdsOnlyPrimaryModifier`
(`lib/shortcuts/primary-modifier.ts`), a registry command, or a per-platform table. The search dialog's "any modified
Enter does nothing" guard reads every modifier flag on purpose.

These still read `metaKey` (alone, or `metaKey || ctrlKey` as "either") to mean the platform's primary modifier. None is
filed yet:

- The browser demo (`lib/demo/`), whose drawn menu bar is always the macOS one: `os-drop.ts:174` and
  `drag-session.ts:43` read `metaKey` as the copy-drag modifier for every visitor (Ctrl on a Windows or Linux visitor's
  real file manager); `context-menu.ts:91` and `menu-bar.svelte.ts:217` skip a popup accelerator on `metaKey`; the
  `onlyControl` checks in `menu-bar.svelte.ts:233` and `windows.ts:293` read Ctrl alone as macOS's ⌃. They follow the
  demo's bar, which follows the visitor's OS only once that's decided (task 63 of `docs/specs/windows-port-plan.md`).
