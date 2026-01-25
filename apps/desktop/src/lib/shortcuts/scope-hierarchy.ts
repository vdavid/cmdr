/**
 * Scope hierarchy for keyboard shortcuts.
 * Determines which scopes' shortcuts are active in a given context.
 * See docs/specs/shortcut-settings.md §2 for specification.
 */

/** All available command scopes */
export type CommandScope =
    | 'App' // Global, works everywhere
    | 'Main window' // Main window context
    | 'File list' // File list focused
    | 'Command palette' // Command palette open
    | 'Navigation' // Navigation context
    | 'Selection' // Selection operations
    | 'Edit' // Edit operations
    | 'View' // View operations
    | 'Help' // Help operations
    | 'About window' // About window context
    | 'Settings window' // Settings window context

/**
 * Scope hierarchy - when a scope is active, these scopes' shortcuts also trigger.
 * Order matters: more specific scopes are listed first for priority.
 */
const scopeHierarchy: Record<CommandScope, CommandScope[]> = {
    App: ['App'],
    'Main window': ['Main window', 'App'],
    'File list': ['File list', 'Main window', 'App'],
    'Command palette': ['Command palette', 'Main window', 'App'],
    Navigation: ['Navigation', 'Main window', 'App'],
    Selection: ['Selection', 'Main window', 'App'],
    Edit: ['Edit', 'Main window', 'App'],
    View: ['View', 'Main window', 'App'],
    Help: ['Help', 'Main window', 'App'],
    'About window': ['About window', 'App'],
    'Settings window': ['Settings window', 'App'],
}

/**
 * Get all scopes that are active when the given scope is current.
 * Returns scopes in priority order (most specific first).
 */
export function getActiveScopes(current: CommandScope): CommandScope[] {
    return scopeHierarchy[current]
}

/**
 * Check if two scopes overlap in the hierarchy.
 * Used for conflict detection - overlapping scopes can have conflicts.
 */
export function scopesOverlap(scopeA: CommandScope, scopeB: CommandScope): boolean {
    const activeA = getActiveScopes(scopeA)
    const activeB = getActiveScopes(scopeB)
    // They overlap if either hierarchy includes the other scope
    return activeA.includes(scopeB) || activeB.includes(scopeA)
}

/** Get all available scopes for display/iteration */
export function getAllScopes(): CommandScope[] {
    return Object.keys(scopeHierarchy) as CommandScope[]
}
