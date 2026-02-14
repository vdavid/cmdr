/**
 * Keyboard handler for shortcut matching.
 */

import { commands } from '$lib/commands/command-registry'
import type { Command } from '$lib/commands/types'
import { formatKeyCombo, isModifierKey } from './key-capture'
import { getActiveScopes, type CommandScope } from './scope-hierarchy'
import { getEffectiveShortcuts } from './shortcuts-store'

/**
 * Get all commands in a specific scope.
 */
function getCommandsInScope(scope: CommandScope): Command[] {
    return commands.filter((c) => c.scope === scope)
}

/**
 * Handle a keyboard event and return the command ID to execute, if any.
 * Returns null if no matching command is found.
 *
 * @param event - The keyboard event
 * @param currentScope - The current active scope
 * @returns The command ID to execute, or null
 */
export function handleKeyDown(event: KeyboardEvent, currentScope: CommandScope): string | null {
    // Ignore pure modifier key presses
    if (isModifierKey(event.key)) {
        return null
    }

    const shortcut = formatKeyCombo(event)
    const activeScopes = getActiveScopes(currentScope)

    // Check scopes in priority order (most specific first)
    for (const scope of activeScopes) {
        const scopeCommands = getCommandsInScope(scope)

        for (const command of scopeCommands) {
            const shortcuts = getEffectiveShortcuts(command.id)

            if (shortcuts.includes(shortcut)) {
                return command.id
            }
        }
    }

    return null
}

/**
 * Find all commands that match a given shortcut, regardless of scope.
 * Useful for the keyboard shortcuts settings UI.
 */
export function findCommandsWithShortcut(shortcut: string): Command[] {
    return commands.filter((cmd) => {
        const shortcuts = getEffectiveShortcuts(cmd.id)
        return shortcuts.includes(shortcut)
    })
}
