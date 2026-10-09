// The first-connect indexing prompt (D6): decide whether to ask, the first time
// the user opens a new external drive this session, if they'd like to index it.
//
// Gating (all must hold): drive indexing is on globally (`indexing.enabled`),
// the per-drive prompt is on (`indexing.askForEachDrive`), this drive isn't
// silenced, the drive isn't already indexed, and it's an external drive (not the
// local `root`, which auto-indexes). A session-level "already prompted" set
// keeps a dismiss-without-choosing from re-prompting on every reselect; the
// persisted silence handles the cross-session case.
//
// Whether the volume can be indexed AT ALL is the caller's gate, not this
// module's: `VolumeBreadcrumb` asks only for a row `isDriveRow` passes, which
// drops every volume no drive index can serve (a server, a view inside a drive)
// on the typed `canBeIndexed` capability.

import { addToast, dismissToast, getToasts } from '$lib/ui/toast'
import { getVolumeIndexStatusById } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { getAppLogger } from '$lib/logging/logger'
import { answersNow, type VolumePresence } from '$lib/file-explorer/navigation/connection-state'
import { isDriveSilenced } from './drive-index-prefs'
import FirstConnectIndexToastContent from './FirstConnectIndexToastContent.svelte'

const log = getAppLogger('indexing')

/** Drives prompted this session, so a reselect doesn't re-nag. */
const promptedThisSession = new Set<string>()

const TOAST_GROUP = 'index-first-connect'

/** The offer on screen for `volumeId`, so it can be withdrawn when its drive goes. */
const toastIdFor = (volumeId: string): string => `${TOAST_GROUP}:${volumeId}`

/** Drives whose offer is (or may still be) on screen. Plain: nothing renders it. */
const offered = new Set<string>()

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

  offered.add(volumeId)
  addToast(FirstConnectIndexToastContent, {
    id: toastIdFor(volumeId),
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

/**
 * Whether a picked drive is ready for the prompt: its session is live (a local
 * drive has none to wait for) and the pane has landed on it.
 *
 * ❗ Picking a saved share only starts its connect, and a sign-in may stand in
 * between, so asking at the pick put "Index private?" over a share that was
 * still connecting, behind its sign-in sheet (QA round 3).
 */
export function isReadyForFirstConnectPrompt(volume: VolumePresence, containingVolumeId: string | null): boolean {
  return answersNow(volume) && containingVolumeId === volume.id
}

/**
 * Withdraws the offer for every drive that left `volumes` or stopped answering (an
 * ejected share, a saved one whose session went). ❗ An offer to index a drive that
 * isn't there any more is a button that can't work: "Index private on
 * localhost:11481?" stayed up after its share was ejected.
 *
 * ❗ Withdrawn, the drive still counts as offered this session: it came back on every
 * reconnect of the same share, which nags about a question the person already saw.
 */
export function withdrawGonePrompts(volumes: readonly VolumePresence[]): void {
  for (const volumeId of offered) {
    const volume = volumes.find((v) => v.id === volumeId)
    if (volume && answersNow(volume)) continue
    offered.delete(volumeId)
    const id = toastIdFor(volumeId)
    if (!getToasts().some((toast) => toast.id === id)) continue
    dismissToast(id)
  }
}
