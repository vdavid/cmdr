/**
 * What `selection.selectSameKind` would select right now, published by the
 * FOCUSED pane so surfaces outside the explorer can label the command with it.
 *
 * Two readers, ONE source: the command palette's row and the Select menu's item
 * both say "Select all with extension *.pdf" rather than the static "Select all
 * of the same kind", so a user sees the effect before running it. ❌ Never add a
 * second source of truth — a label and a selection that can disagree is the one
 * failure mode this module exists to make unrepresentable.
 *
 * - The PALETTE pulls: the registry's `displayName` resolver reads
 *   `sameKindCommandLabel()` at render time, and `CommandPalette`'s `$derived`
 *   re-renders when the cursor moves, since the value below is `$state`.
 * - The MENU BAR is pushed to, because a native item can't read Svelte state.
 *   `publishSameKindTarget` debounces that push (see `PUSH_DEBOUNCE_MS`).
 *
 * ❗ The COMMAND never reads this. It re-reads the cursor row and the pane's
 * whole-listing snapshot when it runs, so a label that is a frame behind can't
 * change what gets selected. This is a label, not a decision. The right-click
 * menu doesn't read it either: it composes its own label at popup time, live.
 *
 * Only the focused pane writes: `FilePane`'s effect bails when `isFocused` is
 * false, so the blurred pane can't clobber the value on a pane switch (both
 * effects re-run, and only one of them writes, whatever order they run in).
 */

import { tString } from '$lib/intl/messages.svelte'
import { updateSelectSameKindMenu } from '$lib/tauri-commands'
import { createDebounce } from '$lib/utils/timing'
import { getAppLogger } from '$lib/logging/logger'
import type { SameKindTarget } from './select-same-kind'

/**
 * How long the cursor has to sit still before the menu bar is told about it.
 *
 * A DEBOUNCE, ❌ never a throttle: holding an arrow key down is a burst whose only
 * interesting value is the last one, and each push is an IPC plus (on macOS) a
 * main-thread AppKit pass to redraw the item's attributed title. 200 ms is short
 * enough that `⌃⏎` can't realistically open the context menu on a stale label —
 * which the mouse trip used to hide, and no longer does.
 */
const PUSH_DEBOUNCE_MS = 200
const log = getAppLogger('sameKindTarget')

let target = $state<SameKindTarget | null>(null)
/** The target the menu bar was last told about, so an unchanged one costs nothing. */
let pushed: SameKindTarget | null = null

/**
 * Tell the menu bar, and remember we did.
 *
 * Fire-and-forget: a menu LABEL that didn't land is worth a log line, never a throw
 * into whatever moved the cursor.
 */
function push(): void {
  pushed = target
  void updateSelectSameKindMenu(target).catch((error: unknown) => {
    log.warn('Could not update the Select menu label: {error}', { error: String(error) })
  })
}

const pushToMenuBar = createDebounce(() => {
  if (sameKindTargetsMatch(pushed, target)) return
  push()
}, PUSH_DEBOUNCE_MS)

/** Whether two targets would render the same words, which is all the push cares about. */
function sameKindTargetsMatch(a: SameKindTarget | null, b: SameKindTarget | null): boolean {
  if (a === null || b === null) return a === b
  if (a.kind !== b.kind) return false
  return a.kind === 'sameExtension' && b.kind === 'sameExtension' ? a.extension === b.extension : true
}

/**
 * Publish the focused pane's current target. Call it with `null` for a row with
 * no kind (the `..` row, an empty listing, a cursor entry still resolving).
 */
export function publishSameKindTarget(next: SameKindTarget | null): void {
  target = next
  pushToMenuBar.call()
}

/**
 * Push the current target at the menu bar again, right now.
 *
 * For a language change: `rebuild_menu_bar` throws the old items away, so the new
 * one comes up with the neutral label and Rust has no idea what the cursor is on.
 * `DualPaneExplorer`'s `menu-bar-rebuilt` handler calls this, beside the other
 * re-pushes of things only the frontend knows.
 */
export function resyncSameKindMenu(): void {
  // A pending push would be redundant with the one below, and firing it late would
  // cost a second AppKit pass for the same words.
  pushToMenuBar.cancel()
  push()
}

/** The focused pane's target, or `null`. Reactive. */
export function getSameKindTarget(): SameKindTarget | null {
  return target
}

/**
 * What the command palette calls `selection.selectSameKind` right now. Falls
 * back to the command's static name when the cursor row implies no target, which
 * is also what Settings > Shortcuts and the help window always show.
 */
export function sameKindCommandLabel(): string {
  const current = target
  if (!current) return tString('commands.selectionSelectSameKind.label')
  switch (current.kind) {
    case 'allFolders':
      return tString('commands.selectionSelectSameKind.allFolders')
    case 'noExtension':
      return tString('commands.selectionSelectSameKind.noExtension')
    case 'sameExtension':
      return tString('commands.selectionSelectSameKind.sameExtension', { extension: current.extension })
  }
}

/** Test-only reset back to "no target", so one spec can't leak a label or a pending push into the next. */
export function _resetForTesting(): void {
  pushToMenuBar.cancel()
  target = null
  pushed = null
}
