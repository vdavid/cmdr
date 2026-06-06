/**
 * Scope hierarchy for keyboard shortcuts.
 * Determines which scopes' shortcuts are active in a given context.
 */

import type { CommandScope } from '$lib/commands/types'

export type { CommandScope }

/**
 * Scope hierarchy - when a scope is active, these scopes' shortcuts also trigger.
 * Each entry is the scope's ancestry chain, most specific first. Two scopes
 * "overlap" (and so can conflict) when one chain contains the other.
 *
 * The chains mirror what actually renders together in the running app:
 *
 * - `Main window/Brief mode` / `Main window/Full mode` sit UNDER
 *   `Main window/File list`: the file list renders in both view modes, so a
 *   mode-scoped key genuinely collides with a File-list key. Brief and Full stay
 *   siblings (neither chain contains the other), so they don't conflict with each
 *   other — the registry binds ←/→ in both on purpose, and the modes never coexist.
 * - `Main window/Network`, `Main window/Share browser`, and
 *   `Main window/Volume chooser` are siblings of `Main window/File list`: a pane
 *   shows one of them INSTEAD of the file list, so their keys don't collide with
 *   File-list keys (they share only `Main window` + `App`).
 *
 * `Command palette` inherits `Main window` (it overlays the main window, so its
 * keys can collide with Main-window keys). `Onboarding` is a modal under `App`
 * only. (The registry has no `Onboarding` commands today; the chain is here so
 * the union stays exhaustive.)
 */
const scopeHierarchy: Record<CommandScope, CommandScope[]> = {
  App: ['App'],
  'Main window': ['Main window', 'App'],
  'Main window/File list': ['Main window/File list', 'Main window', 'App'],
  'Main window/Brief mode': ['Main window/Brief mode', 'Main window/File list', 'Main window', 'App'],
  'Main window/Full mode': ['Main window/Full mode', 'Main window/File list', 'Main window', 'App'],
  'Main window/Network': ['Main window/Network', 'Main window', 'App'],
  'Main window/Share browser': ['Main window/Share browser', 'Main window', 'App'],
  'Main window/Volume chooser': ['Main window/Volume chooser', 'Main window', 'App'],
  'About window': ['About window', 'App'],
  Onboarding: ['Onboarding', 'App'],
  'Command palette': ['Command palette', 'Main window', 'App'],
}

/**
 * Get all scopes that are active when the given scope is current.
 * Returns scopes in priority order (most specific first).
 * Returns empty array for an unknown scope.
 */
export function getActiveScopes(current: string): CommandScope[] {
  if (current in scopeHierarchy) {
    return scopeHierarchy[current as CommandScope]
  }
  return []
}

/**
 * Check if two scopes overlap in the hierarchy.
 * Used for conflict detection - overlapping scopes can have conflicts.
 */
export function scopesOverlap(scopeA: string, scopeB: string): boolean {
  const activeA = getActiveScopes(scopeA)
  const activeB = getActiveScopes(scopeB)
  // They overlap if one scope's ancestry chain contains the other.
  // If either scope is unknown (empty chain), treat them as non-overlapping.
  if (activeA.length === 0 || activeB.length === 0) {
    return false
  }
  return activeA.includes(scopeB as CommandScope) || activeB.includes(scopeA as CommandScope)
}

/** Get all available scopes for display/iteration */
export function getAllScopes(): CommandScope[] {
  return Object.keys(scopeHierarchy) as CommandScope[]
}
