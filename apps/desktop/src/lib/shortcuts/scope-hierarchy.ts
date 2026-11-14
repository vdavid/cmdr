/**
 * Scope hierarchy for keyboard shortcuts.
 * Determines which scopes' shortcuts are active in a given context.
 */

import type { CommandScope } from '#lib/commands/types.js'

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
 * - `Main window/Servers`, `Main window/Places`, `Main window/Volume chooser`,
 *   `Main window/Favorites menu`, and `Main window/Error screen` are siblings of
 *   `Main window/File list`: a pane shows one of them INSTEAD of the file list (or, for the
 *   two header menus, OVER it while central dispatch is suppressed), so their keys don't
 *   collide with File-list keys (they share only `Main window` + `App`). The error screen's ⌘D
 *   is the clearest case — it deliberately shadows whatever ⌘D is bound to
 *   elsewhere, and that's not a conflict to report.
 * - `Main window/Multi-rename` is a sibling too: the ⌃M sheet is a modal over the panes, so
 *   its F2 (the Presets menu) never meets the file list's F2 (Rename). So is
 *   `Main window/Operation log` (⌥⌘L), whose Space, Insert, and ⌘A check items in its tree,
 *   never files in a pane. So is `Main window/Tell me about this`, the card, whose ⌥S (Alt+S)
 *   stops its AI take.
 *
 * `Main window/Ask Cmdr` skips `Main window` on purpose: while the rail has focus its find
 * keys (⌘F, ⌘G, ⇧⌘G) SHADOW Search files and Go to path, and everywhere else those keep
 * their keys. Under `Main window` that deliberate shadow would read as a permanent conflict
 * in Settings, for keys nobody can rebind (they're fixed).
 *
 * `Main window/Search dialog` skips `Main window` for the same reason: the Search and Select
 * files dialogs own the keyboard while they're up (central dispatch is suppressed behind a
 * dialog), so their fixed ⌘N, ⌘H, and ⌘1–⌘3 SHADOW Ask Cmdr's new chat and the view modes
 * and sorts there, by design. The Multi-rename sheet's shape (under `Main window`) would read
 * that shadow as a permanent conflict.
 *
 * `Operation queue window` is a window of its own, like `About window`: its arrows and
 * Enter never meet the main window's, so they share only `App`. So is `Viewer window`, whose
 * bare `W` / `F` / digits never meet the file list's type-to-jump.
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
  'Main window/Servers': ['Main window/Servers', 'Main window', 'App'],
  'Main window/Places': ['Main window/Places', 'Main window', 'App'],
  'Main window/Volume chooser': ['Main window/Volume chooser', 'Main window', 'App'],
  'Main window/Favorites menu': ['Main window/Favorites menu', 'Main window', 'App'],
  'Main window/Error screen': ['Main window/Error screen', 'Main window', 'App'],
  'Main window/Multi-rename': ['Main window/Multi-rename', 'Main window', 'App'],
  'Main window/Operation log': ['Main window/Operation log', 'Main window', 'App'],
  'Main window/Tell me about this': ['Main window/Tell me about this', 'Main window', 'App'],
  'Main window/Ask Cmdr': ['Main window/Ask Cmdr', 'App'],
  'Main window/Search dialog': ['Main window/Search dialog', 'App'],
  'About window': ['About window', 'App'],
  'Operation queue window': ['Operation queue window', 'App'],
  'Viewer window': ['Viewer window', 'App'],
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
