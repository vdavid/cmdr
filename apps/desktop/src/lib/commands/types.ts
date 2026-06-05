/**
 * Command types for the command palette and shortcut configuration.
 */

import type { ViewMode } from '$lib/app-status-store'
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
 * Override slot for the few commands that carry a typed dispatch payload. Each
 * key is the arg-carrying command's id; its value is the payload shape. Later MCP
 * milestones add `sort.set`, `cursor.moveTo`, etc. here without touching the
 * arg-less defaults. PR1 forbids dead arg types, so only commands something
 * actually dispatches with args appear.
 *
 * `view.setMode` is the first: it sets a SPECIFIC pane's view mode (unlike the
 * focused-pane `view.briefMode` / `view.fullMode`), so it needs `{ pane, mode }`.
 * The native-menu `view-mode-changed` event routes here.
 */
export interface CommandArgsOverrides {
  'view.setMode': { pane: 'left' | 'right'; mode: ViewMode }
}

/**
 * Per-command argument map. Commands listed in `CommandArgsOverrides` resolve to
 * their typed payload; every other id resolves to `NoCommandArgs`, so
 * `dispatch('file.rename')` needs no second argument. A conditional mapped type
 * (not an intersection) keeps the arg-less default `undefined` rather than
 * collapsing overridden keys to `never`.
 */
export type CommandArgs = {
  [K in CommandId]: K extends keyof CommandArgsOverrides ? CommandArgsOverrides[K] : NoCommandArgs
}

/**
 * Tuple form of a command's dispatch arguments: `[]` for arg-less commands,
 * `[args]` for arg-carrying ones. Lets a single `dispatch<K>(id, ...args)`
 * signature stay terse for the common case while type-checking the rest.
 *
 * Distributes over `K` (naked type param in the conditional) so a call site
 * holding a broad `CommandId` resolves to `[] | [args]` — i.e. an arg-less
 * dispatch (`handleCommandExecute(commandId)`) stays valid while an arg-carrying
 * literal id (`handleCommandExecute('view.setMode', payload)`) still requires its
 * payload. Without the distribution, `CommandArgs[CommandId]` is a union that
 * doesn't extend `NoCommandArgs`, which would wrongly force a payload on every
 * broad-id call site.
 */
export type CommandDispatchArgs<K extends CommandId> = K extends CommandId
  ? CommandArgs[K] extends NoCommandArgs
    ? []
    : [args: CommandArgs[K]]
  : never

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
