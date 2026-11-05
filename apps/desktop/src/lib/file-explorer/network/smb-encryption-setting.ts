/**
 * The way to "Always require SMB encryption" from a place that refused a share for it: the
 * pane's error screen (a macOS mount that isn't encrypted) and the reconnect view (a server
 * that can't encrypt). Both name the setting, so both offer it (#966).
 */

import { openSettingsWindow, settingAnchorId } from '#lib/settings/settings-window.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('fileExplorer')

/** The setting both refusals point at. */
export const REQUIRE_SMB_ENCRYPTION_SETTING_KEY = 'network.requireSmbEncryption'

/**
 * Deep-links to **Settings > File systems > SMB/Network shares**, scrolled to the "Always
 * require SMB encryption" row. Swallows a failure into a log line: a window that won't open is
 * not worth throwing out of a button that has already said its piece.
 */
export async function openSettingsToSmbEncryption(): Promise<void> {
  try {
    await openSettingsWindow(
      'smb-encryption-refusal',
      ['File systems', 'SMB/Network shares'],
      settingAnchorId(REQUIRE_SMB_ENCRYPTION_SETTING_KEY),
    )
  } catch (err) {
    log.warn('Failed to open Settings from an SMB encryption refusal: {err}', { err })
  }
}
