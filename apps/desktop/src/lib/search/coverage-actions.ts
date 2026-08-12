/**
 * What the coverage note's offer actually does: turn on indexing for the drive a
 * search couldn't cover, and say what happened.
 *
 * Split out of `SearchDialog.svelte` so the wrapper stays glue and the outcome
 * branching is testable on its own. The dialog owns WHETHER to offer (an uncovered
 * gap, a nameable drive, not silenced); this owns what pressing it means.
 */

import { enableDriveIndex } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'
import { addToast } from '$lib/ui/toast'

/**
 * Turn on indexing for `volumeId` and report the outcome as a toast.
 *
 * Branches on the TYPED `EnableIndexingOutcome`, never a message.
 * The master switch outranks every
 * per-drive gate, so "indexing is off globally" needs its own answer or the user
 * presses a button that quietly does nothing. Anything else (an SMB share that needs
 * reconnecting or credentials) points at the drive menu, which offers the same action
 * with fuller guidance rather than duplicating it here.
 */
export async function indexUncoveredDrive(volumeId: string, driveName: string): Promise<void> {
  const drive = driveName || tString('search.coverage.unnamedDrive')
  try {
    const res = await enableDriveIndex(volumeId)
    if (res.status === 'ok' && res.data.status === 'started') {
      addToast(tString('search.coverage.toast.started', { drive }), { level: 'info' })
    } else if (res.status === 'ok' && res.data.status === 'indexing_disabled') {
      addToast(tString('search.coverage.toast.indexingOff'), { level: 'info' })
    } else {
      addToast(tString('search.coverage.toast.notStarted', { drive }), { level: 'warn' })
    }
  } catch {
    addToast(tString('search.coverage.toast.notStarted', { drive }), { level: 'warn' })
  }
}
