/**
 * OS-mount fallback notice bridge.
 *
 * Turns the backend's `smb-fell-back-to-os-mount` event into the persistent INFO
 * toast that offers a retry, and retires that toast once it has nothing left to
 * offer: the share reached a direct connection, or it left the volume list.
 *
 * **Why the dismissal watches `volumes-changed` rather than the retry button.**
 * A share can go direct through four routes: this notice's button, the yellow dot
 * in the breadcrumb, the same item in the breadcrumb dropdown, and the pane's
 * credential form after the user types a working password. All four end in
 * `register_replacing_predecessor`, which broadcasts the volume list, so one rule
 * here covers every route and can't be forgotten by a fifth. A share that goes
 * AWAY (an unmount, an eject, a network drop) broadcasts the list too, and a
 * retry on it could only say it's gone.
 *
 * Mounted from `routes/(main)/+page.svelte` beside the other event bridges. The
 * unsubscribes are returned so the caller can clean up on destroy.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { addToast, dismissToast, getToasts } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { onSmbFellBackToOsMount, onVolumesChanged } from '$lib/tauri-commands'
import type { SmbFellBackToOsMount } from '$lib/ipc/bindings'
import type { VolumeInfo } from '../types'
import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'

const log = getAppLogger('fileExplorer')

/**
 * Per-volume dedup id, shared by the raise and the retire. The backend already
 * speaks once per server per run; this makes a duplicate replace the visible
 * notice in place rather than stack a second one.
 */
export function osMountNoticeToastId(volumeId: string): string {
  return `smb-os-mount:${volumeId}`
}

/** Mounts both listeners. Returns one unsubscribe covering them. */
export async function startOsMountNoticeBridge(): Promise<UnlistenFn> {
  const unlistenFallback = await onSmbFellBackToOsMount(raiseNotice)
  const unlistenVolumes = await onVolumesChanged((payload) => {
    retireMootNotices(payload.data, payload.timedOut)
  })
  log.debug('OS-mount fallback notice bridge mounted')
  return () => {
    unlistenFallback()
    unlistenVolumes()
  }
}

function raiseNotice(payload: SmbFellBackToOsMount): void {
  log.info('Offering a direct-connection retry for the share stuck on the kernel mount: {share}', {
    share: payload.share,
  })
  addToast(SmbOsMountFallbackToastContent, {
    level: 'info',
    dismissal: 'persistent',
    id: osMountNoticeToastId(payload.volumeId),
    closeTooltip: tString('fileExplorer.network.osMountFallback.closeTooltip'),
    props: { volumeId: payload.volumeId, share: payload.share },
  })
}

/**
 * Dismisses every notice whose share now holds a direct session or is no longer
 * listed at all.
 *
 * Asks the toast store which notices are up rather than keeping a list of its
 * own, so there's no ledger here to fall out of step with the backend's or with
 * the user closing one.
 *
 * ❗ A TIMED-OUT listing is the last complete list standing in for a fresh one,
 * so a share missing from it proves nothing: only a listing that finished is
 * allowed to retire a notice by absence.
 */
function retireMootNotices(volumes: VolumeInfo[], listingTimedOut: boolean): void {
  const listed = new Map(volumes.map((volume) => [volume.id, volume]))
  const moot = getToasts().filter((toast) => {
    if (toast.content !== SmbOsMountFallbackToastContent) return false
    const volumeId: unknown = toast.props?.volumeId
    if (typeof volumeId !== 'string') return false
    const volume = listed.get(volumeId)
    return volume === undefined ? !listingTimedOut : volume.connectionState === 'direct'
  })
  // Collected first: dismissing splices the very array being read.
  for (const toast of moot) dismissToast(toast.id)
}
