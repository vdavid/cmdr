/**
 * The combos the OS's text editing owns in a focused text field, per platform: the main window's
 * keydown resolver (`global-keydown.ts`) leaves them to the field whatever command they're bound
 * to. Keyed on the COMBO, never the command it runs today: what collides with typing is the key,
 * whatever a user has bound to it. The sets are `docs/notes/windows-port/keymap-decisions.md` §
 * "Native text keys", combo for combo (`native-text-keys.test.ts` walks them), in canonical
 * `formatKeyCombo` spelling, so a set may hold combos no default binds yet.
 *
 * Checked before dispatch, whatever focus scope is active: off macOS `Ctrl+←` / `Ctrl+→` (copy
 * path between panes) loses to word motion in every main-window field (path bar, quick filter,
 * rename, the Ask Cmdr composer, the search dialog).
 */
import type { Platform } from '#lib/platform/index.js'

export interface NativeTextKeys {
  /** Copy: the browser's own while the page holds a text selection, in a field or not (the error pane). */
  readonly copyWithSelection: ReadonlySet<string>
  /** The focused field's own editing keys: caret and word motion, word and line delete, cut, plain-text paste, its menu. */
  readonly fieldOwned: ReadonlySet<string>
  /**
   * Not native editing, but a command on them would act mid-edit, so they bail in a field too. `Ctrl+I`
   * (Switch pane off macOS) fires from a rename editor and blurs it.
   */
  readonly bailInField: ReadonlySet<string>
}

const MACOS: NativeTextKeys = {
  copyWithSelection: new Set(['⌘C']),
  fieldOwned: new Set([
    // Line start and end, word motion, and their Shift forms. Bare Home and End aren't here: on
    // macOS they scroll and don't move the caret.
    '⌘←',
    '⌘→',
    '⌘⇧←',
    '⌘⇧→',
    '⌥←',
    '⌥→',
    '⌥⇧←',
    '⌥⇧→',
    // Delete to the line start, the word before, and (with ⌦, or fn+⌫) forward. `⌘⌫` and `⌥⌘⌫`
    // are also Move to trash and Delete permanently outside a field.
    '⌘Backspace',
    '⌥Backspace',
    '⌘⌥Backspace',
    '⌘Delete',
    '⌥Delete',
  ]),
  bailInField: new Set(),
}

const WINDOWS_AND_LINUX: NativeTextKeys = {
  copyWithSelection: new Set(['Ctrl+C']),
  fieldOwned: new Set([
    // Copy, which a field answers with or without a page selection.
    'Ctrl+Insert',
    // Word motion and word select.
    'Ctrl+←',
    'Ctrl+→',
    'Ctrl+Shift+←',
    'Ctrl+Shift+→',
    // Line and field start and end, with their select forms.
    'Home',
    'End',
    'Shift+Home',
    'Shift+End',
    'Ctrl+Home',
    'Ctrl+End',
    'Ctrl+Shift+Home',
    'Ctrl+Shift+End',
    // Word delete, and cut (`Shift+Delete`, also Delete permanently outside a field).
    'Ctrl+Backspace',
    'Ctrl+Delete',
    'Shift+Delete',
    // Paste as plain text (Windows, Chromium, and WebKitGTK).
    'Ctrl+Shift+V',
    // The field's own context menu.
    'Shift+F10',
  ]),
  bailInField: new Set(['Ctrl+I']),
}

/** The native text keys of `platform`. */
export function nativeTextKeys(platform: Platform): NativeTextKeys {
  switch (platform) {
    case 'macos':
      return MACOS
    case 'windows':
    case 'linux':
      return WINDOWS_AND_LINUX
  }
}
