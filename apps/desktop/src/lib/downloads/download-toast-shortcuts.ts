/**
 * Pure helper for the downloads toast's collapsed-state shortcut summary.
 *
 * The collapsed toast shows a single line teaching whichever go-to-latest
 * shortcuts are set (in-app `⌘J`, global `⌃⌥⌘J`, or both). This function maps
 * the two snapshotted binding strings to nullable key values; the template
 * renders the surrounding prose and the `ShortcutChip`s around whatever it
 * returns. Keeping the decision pure makes it testable without a DOM.
 *
 * The bridge guarantees the toast only exists when at least one shortcut is
 * set, so at least one of the two fields is always non-null in practice.
 */
export interface ShortcutSummary {
  /** The in-app shortcut key (for example `⌘J`), or `null` when unbound. */
  inApp: string | null
  /** The global from-any-app key (for example `⌃⌥⌘J`), or `null` when off/unbound. */
  global: string | null
}

export function buildShortcutSummary(shortcutHint: string, globalBinding: string): ShortcutSummary {
  return {
    inApp: shortcutHint === '' ? null : shortcutHint,
    global: globalBinding === '' ? null : globalBinding,
  }
}
