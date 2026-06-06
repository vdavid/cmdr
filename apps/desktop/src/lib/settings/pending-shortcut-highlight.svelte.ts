/**
 * Shared state for the deep-link arrival flash in Settings > Keyboard shortcuts.
 *
 * When a `ShortcutChip` (or any other deep-link consumer) opens Settings to a
 * specific command's row via `openShortcutCustomization`, the settings page sets
 * the target command id here; `KeyboardShortcutsSection` reads it to apply a brief
 * highlight on the matching row, then clears it once the animation ends.
 *
 * Why module-level `$state` rather than passing a prop: the section is mounted
 * several layers below the settings page through `SettingsContent`, and the deep
 * link can arrive either on cold-open (URL anchor) or on an already-open window
 * (the `navigate-to-section` event), so a shared module is the clean seam.
 *
 * Why state-driven (not a direct DOM class): the rows re-key on
 * `shortcutChangeCounter` (`{#each … (`${command.id}-${counter}`)}`), so a DOM
 * class set imperatively would vanish on the next re-render. A `class:` directive
 * bound to this state survives re-keying.
 *
 * Both ends MUST import this module (the page writes, the section reads) — knip
 * fails the suite on an export that's only written or only read.
 */

/** The command id whose row should flash, or `null` when nothing is pending. */
let highlightedCommandId = $state<string | null>(null)

/** Read the command id whose row should currently flash. */
export function getPendingShortcutHighlight(): string | null {
  return highlightedCommandId
}

/** Mark a command's row to flash on the next render (deep-link arrival). */
export function setPendingShortcutHighlight(commandId: string): void {
  highlightedCommandId = commandId
}

/** Clear the flash once the animation has played out. */
export function clearPendingShortcutHighlight(): void {
  highlightedCommandId = null
}

/**
 * The section registers a callback that resets its local filters (the name
 * search, the key filter, and the modified/conflicts chip) so a deep link to a
 * row that a leftover filter would hide first makes the row renderable.
 *
 * The page calls `resetShortcutFilters()` synchronously BEFORE its `await tick()`
 * so the row is in the DOM by the time the page scrolls to it. The section owns
 * the actual filter `$state`, so it registers the resetter rather than the page
 * reaching into the section's internals.
 */
let resetFiltersCallback: (() => void) | null = null

/** Called by `KeyboardShortcutsSection` on mount to register its filter resetter. */
export function registerShortcutFilterReset(reset: () => void): void {
  resetFiltersCallback = reset
}

/** Called by `KeyboardShortcutsSection` on unmount to clear the registration. */
export function unregisterShortcutFilterReset(reset: () => void): void {
  if (resetFiltersCallback === reset) {
    resetFiltersCallback = null
  }
}

/** Reset the Keyboard-shortcuts filters if the section is mounted. No-op otherwise. */
export function resetShortcutFilters(): void {
  resetFiltersCallback?.()
}
