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
import type { MessageKey } from '$lib/intl/keys.gen'
import { tString } from '$lib/intl/messages.svelte'

/** A titled group of commands sharing one scope, ready to render. */
export interface ShortcutGroup {
  /** The scope this group renders (stable key for the `{#each}`). */
  scope: CommandScope
  /** User-facing group heading (sentence case). */
  title: string
  commands: Command[]
}

/**
 * Display order + heading KEY for each scope. Reads top-down the way a user
 * scans: global app commands first, then the main window and its inner contexts
 * (file list is the workhorse, so it leads), then the auxiliary windows. The
 * heading copy lives in `messages/en/shortcuts.json` (`shortcuts.scope.*`) and
 * resolves through `tString()` in `groupCommandsByScope`.
 *
 * Keep this in sync with `CommandScope` in `lib/commands/types.ts`: the
 * exhaustiveness test (`union of grouped commands === all registry commands`)
 * fails if a new scope is added without an entry here.
 */
const scopeOrder: readonly { scope: CommandScope; titleKey: MessageKey }[] = [
  { scope: 'App', titleKey: 'shortcuts.scope.app' },
  { scope: 'Main window', titleKey: 'shortcuts.scope.mainWindow' },
  { scope: 'Main window/File list', titleKey: 'shortcuts.scope.fileList' },
  { scope: 'Main window/Brief mode', titleKey: 'shortcuts.scope.briefMode' },
  { scope: 'Main window/Full mode', titleKey: 'shortcuts.scope.fullMode' },
  { scope: 'Main window/Volume chooser', titleKey: 'shortcuts.scope.volumeChooser' },
  { scope: 'Main window/Network', titleKey: 'shortcuts.scope.network' },
  { scope: 'Main window/Share browser', titleKey: 'shortcuts.scope.shareBrowser' },
  { scope: 'Command palette', titleKey: 'shortcuts.scope.commandPalette' },
  { scope: 'About window', titleKey: 'shortcuts.scope.aboutWindow' },
  { scope: 'Onboarding', titleKey: 'shortcuts.scope.onboarding' },
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
  for (const { scope, titleKey } of scopeOrder) {
    const scopeCommands = commands.filter((c) => c.scope === scope)
    if (scopeCommands.length > 0) {
      groups.push({ scope, title: tString(titleKey), commands: scopeCommands })
    }
  }
  return groups
}

/** Every scope the grouping renders, for the exhaustiveness test. */
export const groupedScopes: readonly CommandScope[] = scopeOrder.map((s) => s.scope)
