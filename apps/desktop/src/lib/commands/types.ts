/**
 * Command types for the command palette and shortcut configuration.
 */

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

/** A command definition */
export interface Command {
    /** Unique identifier (e.g., 'file.open', 'nav.parent') */
    id: string
    /** Display name shown in palette */
    name: string
    /** Hierarchical scope */
    scope: CommandScope
    /** Show in command palette? (false for low-level nav like ↑/↓) */
    showInPalette: boolean
    /** Keyboard shortcuts (e.g., ['⌘⇧P', 'F1']) */
    shortcuts: string[]
    /** Optional description for long-form help */
    description?: string
}

/** Result of a fuzzy search match */
export interface CommandMatch {
    /** The matched command */
    command: Command
    /** Indices of matched characters in command.name for highlighting */
    matchedIndices: number[]
}
