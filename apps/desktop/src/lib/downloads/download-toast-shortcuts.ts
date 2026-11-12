/**
 * Pure helper for the downloads toast's collapsed-state shortcut summary.
 *
 * The collapsed toast shows a single line teaching whichever go-to-latest
 * shortcuts are set (in-app `⌘J`, global `⌃⌥⌘J`, or both). This function maps
 * the in-app command's bound state and the snapshotted global binding to the
 * summary's shape; the template renders the surrounding prose and the
 * `ShortcutChip`s (a live `commandId` chip for in-app, a `key` chip for the
 * global hotkey, which no command owns). Keeping the decision pure makes it
 * testable without a DOM.
 *
 * The bridge guarantees the toast only exists when at least one shortcut is
 * set, so at least one of the two fields is always set in practice.
 */
export interface ShortcutSummary {
  /** Whether the in-app `downloads.goToLatest` command has a binding. */
  inApp: boolean
  /** The global from-any-app key (for example `⌃⌥⌘J`), or `null` when off/unbound. */
  global: string | null
}

export function buildShortcutSummary(inAppBound: boolean, globalBinding: string): ShortcutSummary {
  return {
    inApp: inAppBound,
    global: globalBinding === '' ? null : globalBinding,
  }
}
