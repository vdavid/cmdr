/**
 * Centralized shortcut dispatch: reverse lookup from shortcut strings to command IDs.
 *
 * Builds a Map<shortcutString, commandId> for Tier 1 commands (those eligible for
 * central keyboard dispatch). Rebuilds automatically when custom shortcuts change.
 */

import { commands } from '$lib/commands/command-registry'
import type { CommandId } from '$lib/commands'
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'

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

/** Build the reverse lookup map from scratch. */
function buildShortcutMap(): Map<string, CommandId> {
  const map = new Map<string, CommandId>()

  for (const command of commands) {
    if (!isTier1(command)) continue

    const shortcuts = getEffectiveShortcuts(command.id)
    for (const shortcut of shortcuts) {
      // First match wins; skip if shortcut already claimed
      if (!map.has(shortcut)) {
        map.set(shortcut, command.id)
      }
    }
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
