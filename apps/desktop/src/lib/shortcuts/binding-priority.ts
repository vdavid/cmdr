/**
 * A key the user bound themselves beats a default.
 *
 * When two commands in overlapping scopes hold one combo, and one holds it through the
 * user's own binding while the other holds it only by default, the default is SHADOWED:
 * every reader that resolves a combo to a command (the dispatch map, the local matchers,
 * the menu accelerators) treats it as absent. Two user bindings, or two defaults, keep the
 * dispatch map's usual order (deepest scope, then registry order).
 *
 * Without this, a default added in a later release takes a key from a user who had bound it
 * to something else (a macOS user's own `⌥←` lost to the new `nav.back` default).
 *
 * "The user's own binding" is a combo in the command's custom entry that ISN'T among its
 * defaults: a customized command that kept a default key still holds that key by default.
 * A fixed-key or macOS-native command's default is never shadowed: the user can't rebind
 * those, and Settings refuses to give their keys away (Cancel only).
 *
 * The rule reads the store through `BindingReader` so the store (menu accelerators) and the
 * dispatcher (which imports the store) share one implementation without an import cycle.
 */

import { commands } from '#lib/commands/command-registry.js'
import { scopesOverlap } from './scope-hierarchy'

/** The store reads the rule needs. */
export interface BindingReader {
  effective: (commandId: string) => string[]
  defaults: (commandId: string) => string[]
  /** Every command with a custom entry. */
  customizedIds: () => Iterable<string>
  /** False for the fixed-key and macOS-native commands. */
  isRebindable: (commandId: string) => boolean
}

let scopeById: Map<string, string> | null = null

function scopeOf(commandId: string): string | undefined {
  scopeById ??= new Map(commands.map((command) => [command.id, command.scope]))
  return scopeById.get(commandId)
}

/** A customized command holds `combo` through a key the user added, not one of its defaults. */
function holdsByUserBinding(reader: BindingReader, commandId: string, combo: string): boolean {
  return reader.effective(commandId).includes(combo) && !reader.defaults(commandId).includes(combo)
}

/**
 * True when `commandId` holds `combo` by default and another command in an overlapping scope
 * holds it through the user's own binding, so the keypress means that other command.
 */
export function isShadowedDefault(reader: BindingReader, commandId: string, combo: string): boolean {
  if (!reader.isRebindable(commandId)) return false
  const scope = scopeOf(commandId)
  if (scope === undefined) return false
  if (holdsByUserBinding(reader, commandId, combo)) return false
  for (const otherId of reader.customizedIds()) {
    if (otherId === commandId) continue
    const otherScope = scopeOf(otherId)
    if (otherScope === undefined || !scopesOverlap(scope, otherScope)) continue
    if (holdsByUserBinding(reader, otherId, combo)) return true
  }
  return false
}

/** The first of `commandId`'s effective shortcuts no user binding shadows: the one a menu shows. */
export function firstLiveShortcut(reader: BindingReader, commandId: string): string | undefined {
  return reader.effective(commandId).find((combo) => combo !== '' && !isShadowedDefault(reader, commandId, combo))
}
