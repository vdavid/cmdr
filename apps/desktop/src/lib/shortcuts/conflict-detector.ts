/**
 * Conflict detection for keyboard shortcuts.
 * See docs/specs/shortcut-settings.md §5 for specification.
 */

import { commands } from '$lib/commands/command-registry'
import type { Command } from '$lib/commands/types'
import type { ShortcutConflict } from './types'
import { scopesOverlap, type CommandScope } from './scope-hierarchy'
import { getEffectiveShortcuts } from './shortcuts-store'

/**
 * Find commands that conflict with a given shortcut in a given scope.
 * Two commands conflict if they share a shortcut and their scopes overlap.
 */
export function findConflictsForShortcut(shortcut: string, scope: CommandScope, excludeCommandId?: string): Command[] {
    // Empty shortcuts can't conflict
    if (!shortcut) return []

    return commands.filter((cmd) => {
        // Don't conflict with self
        if (cmd.id === excludeCommandId) return false

        // Check if this command uses the shortcut (ignore empty shortcuts)
        const cmdShortcuts = getEffectiveShortcuts(cmd.id).filter((s) => s)
        if (!cmdShortcuts.includes(shortcut)) return false

        // Check if scopes overlap
        return scopesOverlap(cmd.scope as CommandScope, scope)
    })
}

/**
 * Check if a command has any conflicts with other commands.
 */
export function hasConflicts(commandId: string): boolean {
    const command = commands.find((c) => c.id === commandId)
    if (!command) return false

    const shortcuts = getEffectiveShortcuts(commandId)
    const scope = command.scope as CommandScope

    for (const shortcut of shortcuts) {
        const conflicts = findConflictsForShortcut(shortcut, scope, commandId)
        if (conflicts.length > 0) return true
    }

    return false
}

/**
 * Get all conflicts in the system.
 * Returns a list of shortcuts that are bound to multiple overlapping commands.
 */
export function getAllConflicts(): ShortcutConflict[] {
    const conflicts: ShortcutConflict[] = []
    const processed = new Set<string>()

    for (const cmd of commands) {
        // Filter out empty shortcuts (used during editing)
        const shortcuts = getEffectiveShortcuts(cmd.id).filter((s) => s)
        const scope = cmd.scope as CommandScope

        for (const shortcut of shortcuts) {
            // Create a unique key for this shortcut
            const conflictKey = shortcut

            // Skip if we've already processed this shortcut
            if (processed.has(conflictKey)) continue

            // Find all commands using this shortcut with overlapping scopes
            const conflictingCommands = findConflictsForShortcut(shortcut, scope)

            // Add current command if it uses this shortcut
            if (!conflictingCommands.find((c) => c.id === cmd.id)) {
                conflictingCommands.push(cmd)
            }

            // If more than one command, we have a conflict
            if (conflictingCommands.length > 1) {
                // Check for actual scope overlap between all pairs
                const actualConflicts: Command[] = []
                for (const c of conflictingCommands) {
                    const cScope = c.scope as CommandScope
                    // Check if this command conflicts with any other
                    const hasOverlap = conflictingCommands.some(
                        (other) => other.id !== c.id && scopesOverlap(cScope, other.scope as CommandScope),
                    )
                    if (hasOverlap) {
                        actualConflicts.push(c)
                    }
                }

                if (actualConflicts.length > 1) {
                    conflicts.push({
                        shortcut,
                        commandIds: actualConflicts.map((c) => c.id),
                    })
                }

                processed.add(conflictKey)
            }
        }
    }

    return conflicts
}

/**
 * Get the count of commands with conflicts.
 */
export function getConflictCount(): number {
    const conflictingCommands = new Set<string>()

    for (const conflict of getAllConflicts()) {
        for (const id of conflict.commandIds) {
            conflictingCommands.add(id)
        }
    }

    return conflictingCommands.size
}

/**
 * Get all command IDs that have conflicts.
 */
export function getConflictingCommandIds(): Set<string> {
    const ids = new Set<string>()

    for (const conflict of getAllConflicts()) {
        for (const id of conflict.commandIds) {
            ids.add(id)
        }
    }

    return ids
}
