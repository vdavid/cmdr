/**
 * The demo's browser-safe shortcut map, one arm per visitor OS: the keys a browser takes before
 * the page sees them come off, so no hint in the demo names a key that can't reach it. Seeded as
 * user customizations in `shortcuts.json`, the same delta the Settings editor writes, so the app's
 * shortcut system needs no demo awareness and every hint (menus, palette, function bar) shows the
 * remapped combo.
 *
 * - macOS: every default the browser takes (new tab, close tab, reopen tab, new window, switch to
 *   tab N) moves to Control, which browsers on macOS leave alone. PLACEHOLDER: David decides the
 *   real one (issue #302).
 * - Windows and Linux: the registry's own defaults for that OS, minus the combos on
 *   {@link BROWSER_WITHHELD}. Nothing is invented: a command whose every key the browser keeps is
 *   seeded with no key.
 */
import type { CommandId } from '#lib/commands/command-ids.js'
import { commands, defaultShortcuts } from '#lib/commands/command-registry.js'
import type { Platform } from '#lib/platform/index.js'

/** A seeded customization per command: the whole list, as the Settings editor stores one. */
export type BrowserSafeMap = Partial<Record<CommandId, string[]>>

const MAC_VISITOR_MAP: BrowserSafeMap = {
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

/**
 * The keys Chromium never hands a page on Windows and Linux, outside full screen: the accelerators
 * of its reserved commands (`kReservedCommandIds`: close tab and window, new tab, window, and
 * incognito window, reopen tab, next and previous tab). Verified in Chromium's source,
 * `chrome/browser/ui/accelerator_table.cc` and `browser_command_controller.cc`
 * (`IsReservedCommandOrKey`) at `c52b7d226b`, 2026-11-12. Every other browser key (`Alt+←` / `Alt+→`,
 * `F5`, `F11`, `Ctrl+1`–`Ctrl+9`, `Ctrl+F`, …) still reaches the page, which may cancel it, so those
 * stay the demo's. Firefox's reserved set is unverified here.
 */
const CHROMIUM_RESERVED_ON_WINDOWS_AND_LINUX: ReadonlySet<string> = new Set([
  'Ctrl+W',
  'Ctrl+F4',
  'Ctrl+Shift+W',
  'Alt+F4',
  'Ctrl+T',
  'Ctrl+N',
  'Ctrl+Shift+N',
  'Ctrl+Shift+T',
  'Ctrl+Tab',
  'Ctrl+Shift+Tab',
  'Ctrl+PageDown',
  'Ctrl+PageUp',
])

/** The combos a browser keeps from the page, per visitor OS, in that OS's spelling. */
export const BROWSER_WITHHELD: Readonly<Record<'windows' | 'linux', ReadonlySet<string>>> = {
  windows: CHROMIUM_RESERVED_ON_WINDOWS_AND_LINUX,
  linux: CHROMIUM_RESERVED_ON_WINDOWS_AND_LINUX,
}

/** The registry's defaults on `platform`, minus what the browser keeps, for every command that holds one. */
function withoutWithheld(platform: 'windows' | 'linux'): BrowserSafeMap {
  const withheld = BROWSER_WITHHELD[platform]
  const map: BrowserSafeMap = {}
  for (const { id } of commands) {
    const defaults = defaultShortcuts(id, platform)
    if (defaults.some((combo) => withheld.has(combo))) map[id] = defaults.filter((combo) => !withheld.has(combo))
  }
  return map
}

const BROWSER_SAFE_MAPS: Readonly<Record<Platform, () => BrowserSafeMap>> = {
  macos: () => MAC_VISITOR_MAP,
  windows: () => withoutWithheld('windows'),
  linux: () => withoutWithheld('linux'),
}

/** The map to seed for a visitor on `platform` (`./platform.ts`'s answer). */
export function browserSafeShortcuts(platform: Platform): BrowserSafeMap {
  return BROWSER_SAFE_MAPS[platform]()
}
