/**
 * Centralized shortcut dispatch: reverse lookup from shortcut strings to command IDs.
 *
 * Builds a Map<shortcutString, commandId> for Tier 1 commands (those eligible for
 * central keyboard dispatch). Rebuilds automatically when custom shortcuts change.
 */

import { commands } from '$lib/commands/command-registry'
import type { CommandId } from '$lib/commands'
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
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
