/**
 * Pure grouping logic for the Keyboard shortcuts section.
 *
 * Every registry command carries a `CommandScope` (`'App'`, `'Main window'`,
 * `'Main window/File list'`, …). The section renders one titled group per scope,
 * in a fixed reading order, so EVERY command shows up in exactly one group and is
 * rebindable through the UI.
 *
 * Group straight off `CommandScope` (don't reintroduce an ad-hoc title list that
 * has to be matched against scopes): commands on compound scopes like
 * `'Main window/File list'` only render because the group set IS the scope union.
 * A title list that drifts from the scopes silently hides whole groups of
 * commands from the rebinding UI.
 */
import type { Command } from '$lib/commands/types'
import type { CommandScope } from '$lib/commands/types'

/** A titled group of commands sharing one scope, ready to render. */
export interface ShortcutGroup {
  /** The scope this group renders (stable key for the `{#each}`). */
  scope: CommandScope
  /** User-facing group heading (sentence case). */
  title: string
  commands: Command[]
}

/**
 * Display order + heading for each scope. Reads top-down the way a user scans:
 * global app commands first, then the main window and its inner contexts (file
 * list is the workhorse, so it leads), then the auxiliary windows.
 *
 * Keep this in sync with `CommandScope` in `lib/commands/types.ts`: the
 * exhaustiveness test (`union of grouped commands === all registry commands`)
 * fails if a new scope is added without an entry here.
 */
const scopeOrder: readonly { scope: CommandScope; title: string }[] = [
  { scope: 'App', title: 'App' },
  { scope: 'Main window', title: 'Main window' },
  { scope: 'Main window/File list', title: 'File list' },
  { scope: 'Main window/Brief mode', title: 'Brief mode' },
  { scope: 'Main window/Full mode', title: 'Full mode' },
  { scope: 'Main window/Volume chooser', title: 'Volume chooser' },
  { scope: 'Main window/Network', title: 'Network' },
  { scope: 'Main window/Share browser', title: 'Share browser' },
  { scope: 'Command palette', title: 'Command palette' },
  { scope: 'About window', title: 'About window' },
  { scope: 'Onboarding', title: 'Onboarding' },
]

/**
 * Group commands by scope into the fixed display order, dropping empty groups.
 *
 * Every command lands in exactly one group (its `scope`); a command whose scope
 * isn't in `scopeOrder` would silently vanish, which the exhaustiveness test
 * guards against.
 */
export function groupCommandsByScope(commands: Command[]): ShortcutGroup[] {
  const groups: ShortcutGroup[] = []
  for (const { scope, title } of scopeOrder) {
    const scopeCommands = commands.filter((c) => c.scope === scope)
    if (scopeCommands.length > 0) {
      groups.push({ scope, title, commands: scopeCommands })
    }
  }
  return groups
}

/** Every scope the grouping renders, for the exhaustiveness test. */
export const groupedScopes: readonly CommandScope[] = scopeOrder.map((s) => s.scope)
