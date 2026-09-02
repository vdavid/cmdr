/**
 * Turning a KeyboardEvent into a command — the one place that answers "what did the
 * user just press?", for the document-level dispatcher and for local handlers alike.
 *
 * Two shapes, because there are two questions:
 *
 * - `lookupCommand`: the GLOBAL reverse lookup over Tier 1 commands
 *   (`Map<shortcutString, commandId>`, one winner per combo). Right when the caller has
 *   no scope of its own — the document keydown handler.
 * - `eventMatchesCommand`: does this keypress match THIS command? Right for a local
 *   handler, which knows its own scope and so must not be handed whichever command
 *   happens to win a combo globally (`Enter` is claimed by five scopes).
 *
 * Both match the WHOLE combo, so a modifier superset can never trigger a handler:
 * `⌥⌘A` is not `⌘A`. Hand-rolled predicates like `e.key === 'a' && e.metaKey` are how
 * "Ask Cmdr" also selected every file.
 */

import { commands } from '$lib/commands/command-registry'
import type { CommandId } from '$lib/commands'
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
import { formatKeyCombo } from './key-capture'
import { getActiveScopes } from './scope-hierarchy'

// Command IDs that have showInPalette: false but still need central dispatch
const ALWAYS_DISPATCH_IDS = new Set<CommandId>(['app.commandPalette'])

let shortcutMap = new Map<string, CommandId>()
let unsubscribe: (() => void) | null = null

/**
 * Check whether a command is Tier 1 (centrally dispatched).
 * Tier 1 = showInPalette OR in the always-dispatch list.
 */
function isTier1(command: { id: CommandId; showInPalette: boolean }): boolean {
  return command.showInPalette || ALWAYS_DISPATCH_IDS.has(command.id)
}

/**
 * Build the reverse lookup map from scratch.
 *
 * When two Tier 1 commands claim the same combo (a kept "Keep both" conflict),
 * the MORE SPECIFIC scope wins — its ancestry chain via `getActiveScopes` is
 * longer — with registry order as the stable tiebreaker for equal specificity.
 * Without the scope rule the winner would be whichever command happens to be
 * declared first in the registry, so an unrelated registry reorder could
 * silently flip a user's binding.
 */
function buildShortcutMap(): Map<string, CommandId> {
  // All claims per combo first, then one deterministic winner per combo.
  const claims = new Map<string, { id: CommandId; depth: number; registryIndex: number }[]>()

  commands.forEach((command, registryIndex) => {
    if (!isTier1(command)) return

    const depth = getActiveScopes(command.scope).length
    for (const shortcut of getEffectiveShortcuts(command.id)) {
      const list = claims.get(shortcut) ?? []
      list.push({ id: command.id, depth, registryIndex })
      claims.set(shortcut, list)
    }
  })

  const map = new Map<string, CommandId>()
  for (const [shortcut, list] of claims) {
    list.sort((a, b) => b.depth - a.depth || a.registryIndex - b.registryIndex)
    map.set(shortcut, list[0].id)
  }

  return map
}

/** Look up which command ID a shortcut string maps to, if any. */
export function lookupCommand(shortcutString: string): CommandId | undefined {
  return shortcutMap.get(shortcutString)
}

/**
 * True when the keypress matches one of `commandId`'s effective shortcuts EXACTLY,
 * modifiers and all. Works for every command, Tier 1 or fixed-key, so a local handler
 * reads its keys from the registry instead of hardcoding them.
 *
 * `allowShift` accepts the same combo with Shift held, for ANY combo (`⌘⇧A` matches
 * `⌘A`, not just `⇧↓` matching `↓`). The file list uses Shift to extend the selection
 * while the cursor moves, so `⇧↓` still has to mean "move down" — while `⌘↓` (open) and
 * `⌥↓` (go to end) stay different commands. Only pass it where Shift genuinely carries
 * that extra meaning.
 */
export function eventMatchesCommand(event: KeyboardEvent, commandId: CommandId, options?: MatchOptions): boolean {
  if (comboMatchesCommand(formatKeyCombo(event), commandId, options)) return true
  const physical = physicalDigitCombo(event)
  return physical !== null && comboMatchesCommand(physical, commandId, options)
}

/**
 * The combo a Shift+digit press would format as if the layout had typed the digit
 * itself. `⇧8` is `*` on US QWERTY and `(` on Hungarian, so `formatKeyCombo` never
 * yields `⇧8` on any layout; matching the physical `Digit8` key makes a `⇧<digit>`
 * default bindable at all, and layout-independent. Only Shift+digit gets this
 * treatment: for every other key `event.key` is the right identity.
 */
function physicalDigitCombo(event: KeyboardEvent): string | null {
  if (!event.shiftKey) return null
  const digit = /^Digit(\d)$/.exec(event.code)?.[1]
  if (digit === undefined || digit === event.key) return null
  return formatKeyCombo({ ...eventModifiers(event), key: digit, code: event.code } as KeyboardEvent)
}

function eventModifiers(event: KeyboardEvent): Pick<KeyboardEvent, 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'> {
  return { metaKey: event.metaKey, ctrlKey: event.ctrlKey, altKey: event.altKey, shiftKey: event.shiftKey }
}

/** Options shared by the two matchers. */
interface MatchOptions {
  allowShift?: boolean
}

/**
 * `eventMatchesCommand` for a combo already produced by `formatKeyCombo`. Use it when
 * one keypress is tested against several commands (the file list's cursor keys), so
 * the event is formatted once.
 */
export function comboMatchesCommand(
  combo: string,
  commandId: CommandId,
  { allowShift = false }: MatchOptions = {},
): boolean {
  const shortcuts = getEffectiveShortcuts(commandId)
  if (shortcuts.includes(combo)) return true
  return allowShift && shortcuts.includes(withoutShift(combo))
}

/**
 * A combo with its Shift modifier removed, wherever it sits in the prefix. Deliberately
 * NOT anchored at the start: `formatKeyCombo` emits modifiers in ⌘⌃⌥⇧ order, so `⇧`
 * leads only when it's the ONLY modifier. An anchored strip would quietly do nothing for
 * `⌘⇧A` and hand the caller a false "no match" — exactly the silent-mismatch class
 * `allowShift` exists to prevent. Unanchored removal is safe because no key NAME
 * contains `⇧` or `Shift+`.
 */
function withoutShift(combo: string): string {
  return combo.replace('⇧', '').replace('Shift+', '')
}

/** Initialize the dispatch map and subscribe to shortcut changes. */
export function initShortcutDispatch(): void {
  shortcutMap = buildShortcutMap()
  unsubscribe = onShortcutChange(() => {
    shortcutMap = buildShortcutMap()
  })
}

/** Tear down: unsubscribe from shortcut changes and clear the map. */
export function destroyShortcutDispatch(): void {
  unsubscribe?.()
  unsubscribe = null
  shortcutMap = new Map<string, CommandId>()
}
