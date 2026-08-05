// The first-connect indexing prompt (D6): decide whether to ask, the first time
// the user opens a new external drive this session, if they'd like to index it.
//
// Gating (all must hold): drive indexing is on globally (`indexing.enabled`),
// the per-drive prompt is on (`indexing.askForEachDrive`), this drive isn't
// silenced, the drive isn't already indexed, and it's an external drive (not the
// local `root`, which auto-indexes). A session-level "already prompted" set
// keeps a dismiss-without-choosing from re-prompting on every reselect; the
// persisted silence handles the cross-session case.

import { addToast } from '$lib/ui/toast'
import { getVolumeIndexStatusById } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { getAppLogger } from '$lib/logging/logger'
import { isDriveSilenced } from './drive-index-prefs'
import FirstConnectIndexToastContent from './FirstConnectIndexToastContent.svelte'

const log = getAppLogger('indexing')

/** Drives prompted this session, so a reselect doesn't re-nag. */
const promptedThisSession = new Set<string>()

const TOAST_GROUP = 'index-first-connect'

export interface FirstConnectActions {
  onEnable: (volumeId: string) => void
  onSilenceDrive: (volumeId: string) => void
  onSilenceAll: () => void
}

/**
 * Show the first-connect prompt for `volumeId` if every gate passes. Safe to
 * call on every drive selection: it self-gates and no-ops otherwise.
 */
export async function maybePromptFirstConnect(
  volumeId: string,
  volumeName: string,
  actions: FirstConnectActions,
): Promise<void> {
  // Local disk auto-indexes (FDA-gated elsewhere); the prompt is for external drives.
  if (volumeId === 'root') return
  if (promptedThisSession.has(volumeId)) return
  if (!getSetting('indexing.enabled')) return
  if (!getSetting('indexing.askForEachDrive')) return
  if (isDriveSilenced(volumeId)) return

  // Don't prompt a drive a scan has already indexed (or is indexing right now).
  // ⚠️ `enabled` alone doesn't say that: a search's walk registers a writer-only
  // instance on a drive nothing has ever scanned, and `freshness` is what tells
  // the two apart (`null` until something scans). Suppressing on `enabled` alone
  // retired the offer for the drive that most needs it — a walk covered the
  // folder somebody searched, not the drive.
  const statusRes = await getVolumeIndexStatusById(volumeId)
  if (statusRes.status === 'ok' && statusRes.data.enabled && statusRes.data.freshness !== null) return

  promptedThisSession.add(volumeId)
  log.debug('Showing first-connect index prompt for {vid}', { vid: volumeId })

  addToast(FirstConnectIndexToastContent, {
    level: 'info',
    dismissal: 'persistent',
    toastGroup: TOAST_GROUP,
    props: {
      volumeId,
      volumeName,
      onEnable: actions.onEnable,
      onSilenceDrive: actions.onSilenceDrive,
      onSilenceAll: actions.onSilenceAll,
    },
  })
}
