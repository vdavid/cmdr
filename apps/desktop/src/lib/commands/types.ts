/**
 * Command types for the command palette and shortcut configuration.
 */

import type { CommandId } from './command-ids'

export type { CommandId } from './command-ids'

/** Command scope hierarchy - determines where command is available */
export type CommandScope =
  | 'App' // Global commands (⌘Q works everywhere)
  | 'Main window' // Main window commands
  | 'Main window/File list' // File list navigation/actions
  | 'Main window/Brief mode' // Brief mode specific
  | 'Main window/Full mode' // Full mode specific
  | 'Main window/Network' // Network browser
  | 'Main window/Share browser' // Share browser
  | 'Main window/Volume chooser' // Volume dropdown
  | 'About window' // About window commands
  | 'Onboarding' // FDA prompt
  | 'Command palette' // Command palette modal

/**
 * Marker for a command that takes no dispatch argument. A distinct named type
 * (not `void`, which `@typescript-eslint/no-invalid-void-type` bans outside
 * return positions) so `CommandArgs` can map arg-less ids to it and
 * `CommandDispatchArgs` can branch on it.
 */
export type NoCommandArgs = undefined

/**
 * Per-command argument map. Arg-less commands resolve to `NoCommandArgs`, so
 * `dispatch('file.rename')` needs no second argument; arg-carrying commands
 * (the per-pane MCP variants landing in later milestones) override their entry
 * with a typed payload.
 *
 * Today every command is arg-less (the dispatch context rides separately), so
 * the map is "all ids → NoCommandArgs". The override slot is where M3/M4 add
 * `sort.set`, `cursor.moveTo`, etc. without touching the arg-less defaults. We
 * intentionally don't pre-populate it: PR1 forbids dead arg types for commands
 * nothing dispatches yet.
 */
export type CommandArgs = {
  [K in CommandId]: NoCommandArgs
}

/**
 * Tuple form of a command's dispatch arguments: `[]` for arg-less commands,
 * `[args]` for arg-carrying ones. Lets a single `dispatch<K>(id, ...args)`
 * signature stay terse for the common case while type-checking the rest.
 */
export type CommandDispatchArgs<K extends CommandId> = CommandArgs[K] extends NoCommandArgs
  ? []
  : [args: CommandArgs[K]]

/** A command definition */
export interface Command {
  /** Unique identifier (like 'file.open', 'nav.parent') */
  id: CommandId
  /** Display name shown in palette */
  name: string
  /** Hierarchical scope */
  scope: CommandScope
  /** Show in command palette? (false for low-level nav like ↑/↓) */
  showInPalette: boolean
  /** Keyboard shortcuts (like ['⌘⇧P', 'F1']) */
  shortcuts: string[]
  /** Optional description for long-form help */
  description?: string
  /**
   * Extra search terms folded into the fuzzy-search haystack so the palette finds the command
   * by synonyms not present in its name (like ['jump', 'navigate']). They never produce visible
   * highlights: matches landing in keyword text are dropped (see `fuzzy-search.ts`).
   */
  keywords?: string[]
}

/** Result of a fuzzy search match */
export interface CommandMatch {
  /** The matched command */
  command: Command
  /** Indices of matched characters in command.name for highlighting */
  matchedIndices: number[]
}
