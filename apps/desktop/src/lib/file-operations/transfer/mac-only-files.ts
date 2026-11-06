/**
 * The "Skip Mac-only hidden files" choice (#314): how the `fileOperations.macOnlyFiles`
 * setting turns into the copy, move, and compress dialog's toggle and link, and into what
 * an operation sends. The backend owns the rest: which names match (`.DS_Store`, `._*`),
 * and leaving them out of every walk, the totals, and a move's source.
 *
 * The toggle is always a one-off override: flipping it never writes the setting. Only the
 * link opens the setting row.
 */

import type { MacOnlyFiles } from '#lib/tauri-commands/index.js'
import type { MacOnlyFilesPolicy } from '#lib/settings/types.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

/** The setting the toggle starts from and its link opens. */
export const MAC_ONLY_FILES_SETTING = 'fileOperations.macOnlyFiles' as const

/** Whether the toggle starts ticked: only a `skip` default ticks it; `ask` starts it off. */
export function macOnlyToggleStartsOn(policy: MacOnlyFilesPolicy): boolean {
  return policy === 'skip'
}

/** What a confirm sends for the toggle's state. */
export function macOnlyFilesFor(skip: boolean): MacOnlyFiles {
  return skip ? 'skip' : 'include'
}

/** What an operation sends when nobody saw the toggle (skipped confirmation, an MCP start
 *  with no dialog): the toggle's starting state, so `ask` includes them. */
export function defaultMacOnlyFiles(policy: MacOnlyFilesPolicy): MacOnlyFiles {
  return macOnlyFilesFor(macOnlyToggleStartsOn(policy))
}

/** The link beside the toggle: an offer to stop asking, or the default it starts from. */
export function macOnlyLinkKey(policy: MacOnlyFilesPolicy): MessageKey {
  switch (policy) {
    case 'include':
      return 'fileOperations.transferDialog.macOnlyFiles.includingByDefault'
    case 'skip':
      return 'fileOperations.transferDialog.macOnlyFiles.skippingByDefault'
    // `ask`, and anything a hand-edited settings file could hold: the dialog asks.
    default:
      return 'fileOperations.transferDialog.macOnlyFiles.dontAskAgain'
  }
}

/** Opens Settings at the "Skip Mac-only hidden files" row. Imported lazily, like the
 *  attach-email link's: a static import would pull the settings window's Tauri surface into
 *  every dialog's module graph. */
export async function openMacOnlyFilesSetting(): Promise<void> {
  const { openSettingsWindow, settingAnchorId } = await import('#lib/settings/settings-window.js')
  // The section path is the setting's registered home (`definitions/behavior.ts`).
  await openSettingsWindow(
    'transfer-dialog',
    ['Behavior', 'Navigation & file ops'],
    settingAnchorId(MAC_ONLY_FILES_SETTING),
  )
}
