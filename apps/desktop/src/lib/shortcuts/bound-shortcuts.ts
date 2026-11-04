/**
 * The registry as one flat map, for the native popup menus to label their items with.
 */

import { COMMAND_IDS } from '#lib/commands/command-ids.js'
import { getEffectiveShortcuts } from './shortcuts-store'

/**
 * Every shortcut bound right now, as `commandId → combo`.
 *
 * A native context menu is rebuilt from scratch on every right-click, so it can label
 * its items from this instead of carrying literal strings, which start lying the moment
 * someone rebinds a key in Settings > Shortcuts. A command with nothing bound is left
 * out, and its item then shows no accelerator, which is the honest answer. Rust looks
 * each one up by MENU id (`cmdr_menu::context`, `ContextMenuShortcuts`).
 *
 * ❗ The canonical spelling, ❌ never `toDisplayShortcut`: the display glyphs (`⌫`, `⎋`)
 * mean nothing to Rust's accelerator conversion and come out as garbage, and macOS draws
 * the canonical names as glyphs by itself.
 *
 * Built fresh per popup, which is what keeps it true. That's one map lookup per command,
 * against a right-click that already spends a LaunchServices query on "Open with".
 *
 * ❗ It lives HERE rather than inside the `showFileContextMenu` wrapper, which is where
 * it belongs by rights: `shortcuts-store` imports `#lib/tauri-commands` to push menu-bar
 * accelerators, so reading the registry from inside that barrel closes a cycle the
 * `import-cycles` check rejects. The callers pass it instead.
 */
export function boundShortcuts(): Record<string, string> {
  const bound: Record<string, string> = {}
  for (const commandId of COMMAND_IDS) {
    const combo = getEffectiveShortcuts(commandId)[0]
    if (combo) bound[commandId] = combo
  }
  return bound
}
