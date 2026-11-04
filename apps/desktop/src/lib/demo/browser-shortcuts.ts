/**
 * The demo's browser-safe shortcut map: every default the browser takes before the page sees it
 * (new tab, close tab, reopen tab, new window, switch to tab N) moves to Control, which browsers
 * on macOS leave alone. Seeded as user customizations in `shortcuts.json`, the same delta the
 * Settings editor writes, so the app's shortcut system needs no demo awareness and every hint
 * (menus, palette, function bar) shows the remapped combo.
 *
 * PLACEHOLDER map: David decides the real one (issue #302).
 */
import type { CommandId } from '#lib/commands/command-ids.js'

export const browserSafeShortcuts: Partial<Record<CommandId, string[]>> = {
  'tab.new': ['⌃T'],
  'tab.close': ['⌃W'],
  'tab.reopen': ['⌃⇧T'],
  'askCmdr.newChat': ['⌃N'],
  'view.fullMode': ['⌃1'],
  'view.briefMode': ['⌃2'],
  'sort.byName': ['⌃3', '⌘F3'],
  'sort.byExtension': ['⌃4', '⌘F4'],
  'sort.byModified': ['⌃5', '⌘F5'],
  'sort.bySize': ['⌃6', '⌘F6'],
}
