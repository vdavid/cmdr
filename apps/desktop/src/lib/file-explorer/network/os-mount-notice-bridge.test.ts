/**
 * Tests for the OS-mount fallback notice bridge: one notice per fallback event,
 * retired the moment the share reports a direct connection, leaves the volume
 * list, or the backend withdraws it (the share's direct connection switched off).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { SmbFellBackToOsMount, SmbOsMountNoticeWithdrawn } from '$lib/ipc/bindings'
import type { VolumeInfo } from '../types'

/**
 * A stand-in toast store that remembers which toasts are up, because the bridge
 * asks the store rather than keeping a list of its own.
 */
const toastStore = vi.hoisted(() => {
  const up: { id: string; content: unknown; props?: Record<string, unknown> }[] = []
  return {
    up,
    addToast: vi.fn((content: unknown, options: Record<string, unknown>) => {
      const id = options.id as string
      const toast = { id, content, props: options.props as Record<string, unknown> | undefined }
      const at = up.findIndex((t) => t.id === id)
      if (at === -1) up.push(toast)
      else up[at] = toast
      return id
    }),
    dismissToast: vi.fn((id: string) => {
      const at = up.findIndex((t) => t.id === id)
      if (at !== -1) up.splice(at, 1)
    }),
    getToasts: () => up,
  }
})
const { addToast, dismissToast } = toastStore
vi.mock('$lib/ui/toast', () => ({
  addToast: toastStore.addToast,
  dismissToast: toastStore.dismissToast,
  getToasts: toastStore.getToasts,
}))

let emitFallback: (payload: SmbFellBackToOsMount) => void
let emitVolumes: (payload: { data: VolumeInfo[]; timedOut: boolean; discoveryPending?: boolean }) => void
const unlistenFallback = vi.fn()
const unlistenVolumes = vi.fn()
let emitWithdrawn: (payload: SmbOsMountNoticeWithdrawn) => void
const unlistenWithdrawn = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  onSmbFellBackToOsMount: (handler: (payload: SmbFellBackToOsMount) => void) => {
    emitFallback = handler
    return Promise.resolve(unlistenFallback)
  },
  onSmbOsMountNoticeWithdrawn: (handler: (payload: SmbOsMountNoticeWithdrawn) => void) => {
    emitWithdrawn = handler
    return Promise.resolve(unlistenWithdrawn)
  },
  onVolumesChanged: (
    handler: (payload: { data: VolumeInfo[]; timedOut: boolean; discoveryPending?: boolean }) => void,
  ) => {
    emitVolumes = handler
    return Promise.resolve(unlistenVolumes)
  },
}))

import { startOsMountNoticeBridge, osMountNoticeToastId } from './os-mount-notice-bridge'
import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'

function volume(id: string, connectionState: VolumeInfo['connectionState']): VolumeInfo {
  return { id, name: id, path: `/Volumes/${id}`, connectionState } as VolumeInfo
}

beforeEach(async () => {
  toastStore.up.length = 0
  addToast.mockClear()
  dismissToast.mockClear()
  await startOsMountNoticeBridge()
})

describe('the OS-mount fallback notice', () => {
  it('names the share and hands the volume to the retry button', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    expect(addToast).toHaveBeenCalledTimes(1)
    const [content, options] = addToast.mock.calls[0]
    expect(content).toBe(SmbOsMountFallbackToastContent)
    expect(options.props).toEqual({ volumeId: 'smb-archive', share: 'archive', retryable: true })
  })

  // The server answered that it has no such share. The same identity asking it
  // again gets the same answer, so the notice explains and offers nothing. Before
  // ERR-HYPZG this arrived as `unexpected` and put a button on screen that could
  // only ever fail.
  it('offers no retry when the server says it has no such share', () => {
    emitFallback({ volumeId: 'smb-gone', share: 'gone', reason: 'shareNotOnServer', displayName: 'Naspolya' })

    const [, options] = addToast.mock.calls[0]
    expect(options.props).toEqual({ volumeId: 'smb-gone', share: 'gone', retryable: false })
  })

  // Every other reason is a condition that can pass on its own, including a DFS
  // namespace whose targets are all down (`unreachable`), so the button stays.
  it('keeps the retry for every reason that can change on its own', () => {
    for (const reason of ['unreachable', 'tooSlow', 'unexpected'] as const) {
      addToast.mockClear()
      emitFallback({ volumeId: `smb-${reason}`, share: 'archive', reason, displayName: 'Naspolya' })

      const [, options] = addToast.mock.calls[0]
      expect(options.props, reason).toMatchObject({ retryable: true })
    }
  })

  // ERR-XGS9X: this Mac refused the route while its own mount of the share worked,
  // so the notice says which server and what to switch, and keeps the retry for
  // after the switch.
  it('names the server it couldn’t connect to when this Mac is what blocked it', () => {
    emitFallback({ volumeId: 'smb-sven', share: 'Sven', reason: 'blockedByThisMac', displayName: 'Mars' })

    const [, options] = addToast.mock.calls[0]
    expect(options.props).toEqual({ volumeId: 'smb-sven', share: 'Sven', retryable: true, blockedServer: 'Mars' })
  })

  it('stays up until the user acts on it, because the share is slow the whole time', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    const [, options] = addToast.mock.calls[0]
    expect(options.dismissal).toBe('persistent')
    expect(options.level).toBe('info')
  })

  it('dedups per volume, so a repeat replaces the notice instead of stacking one', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    const [, options] = addToast.mock.calls[0]
    expect(options.id).toBe(osMountNoticeToastId('smb-archive'))
  })

  it('retires itself once the share reports a direct connection', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    emitVolumes({ data: [volume('smb-archive', 'direct')], timedOut: false })

    expect(dismissToast).toHaveBeenCalledWith(osMountNoticeToastId('smb-archive'))
  })

  it('leaves the notice up while the share is still on the OS mount', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    emitVolumes({ data: [volume('smb-archive', 'os_mount')], timedOut: false })

    expect(dismissToast).not.toHaveBeenCalled()
  })

  it('retires only the share that went direct, not every notice on screen', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })
    emitFallback({ volumeId: 'smb-photos', share: 'photos', reason: 'unexpected', displayName: 'Naspolya' })

    emitVolumes({
      data: [volume('smb-archive', 'direct'), volume('smb-photos', 'os_mount')],
      timedOut: false,
    })

    expect(dismissToast).toHaveBeenCalledExactlyOnceWith(osMountNoticeToastId('smb-archive'))
  })

  it('retires the notice once its share leaves the volume list, since there is nothing left to retry', () => {
    // An unmount, an eject, or a network drop: ERR-SHUSC's button kept offering
    // a retry on a volume that no longer existed.
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })
    emitFallback({ volumeId: 'smb-photos', share: 'photos', reason: 'unexpected', displayName: 'Naspolya' })

    emitVolumes({ data: [volume('smb-photos', 'os_mount')], timedOut: false })

    expect(dismissToast).toHaveBeenCalledExactlyOnceWith(osMountNoticeToastId('smb-archive'))
  })

  it('keeps the notice through a timed-out listing, which may have missed a share that is still there', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    emitVolumes({ data: [], timedOut: true })

    expect(dismissToast).not.toHaveBeenCalled()
  })

  it('keeps the notice through a listing whose discovery is still pending, which carries the cached local part', () => {
    // A hung mount makes the backend publish fresh server rows beside the LAST
    // local listing, which may predate this share's mount.
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })

    emitVolumes({ data: [], timedOut: false, discoveryPending: true })

    expect(dismissToast).not.toHaveBeenCalled()
  })

  it('leaves every other toast alone when a volume leaves the list', () => {
    toastStore.up.push({ id: 'copy-done', content: 'Copied 3 items' })

    emitVolumes({ data: [], timedOut: false })

    expect(dismissToast).not.toHaveBeenCalled()
  })

  // The user switched the share's direct connection off, so the notice's button
  // would do exactly what they just opted out of. The backend withdraws it from
  // the one place every route to the switch passes through.
  it('retires the notice the backend withdraws, and only that one', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive', reason: 'unexpected', displayName: 'Naspolya' })
    emitFallback({ volumeId: 'smb-photos', share: 'photos', reason: 'unexpected', displayName: 'Naspolya' })

    emitWithdrawn({ volumeId: 'smb-archive' })

    expect(dismissToast).toHaveBeenCalledExactlyOnceWith(osMountNoticeToastId('smb-archive'))
  })

  it('unsubscribes every listener together', async () => {
    const unlisten = await startOsMountNoticeBridge()

    unlisten()

    expect(unlistenFallback).toHaveBeenCalled()
    expect(unlistenVolumes).toHaveBeenCalled()
    expect(unlistenWithdrawn).toHaveBeenCalled()
  })
})
