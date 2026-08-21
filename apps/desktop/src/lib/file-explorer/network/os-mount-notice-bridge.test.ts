/**
 * Tests for the OS-mount fallback notice bridge: one notice per fallback event,
 * retired the moment the share reports a direct connection.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { SmbFellBackToOsMount } from '$lib/ipc/bindings'
import type { VolumeInfo } from '../types'

const { addToast, dismissToast } = vi.hoisted(() => ({
  addToast: vi.fn<(content: unknown, options: Record<string, unknown>) => string>(),
  dismissToast: vi.fn<(id: string) => void>(),
}))
vi.mock('$lib/ui/toast', () => ({ addToast, dismissToast }))

let emitFallback: (payload: SmbFellBackToOsMount) => void
let emitVolumes: (payload: { data: VolumeInfo[]; timedOut: boolean }) => void
const unlistenFallback = vi.fn()
const unlistenVolumes = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  onSmbFellBackToOsMount: (handler: (payload: SmbFellBackToOsMount) => void) => {
    emitFallback = handler
    return Promise.resolve(unlistenFallback)
  },
  onVolumesChanged: (handler: (payload: { data: VolumeInfo[]; timedOut: boolean }) => void) => {
    emitVolumes = handler
    return Promise.resolve(unlistenVolumes)
  },
}))

import { startOsMountNoticeBridge, osMountNoticeToastId } from './os-mount-notice-bridge'
import SmbOsMountFallbackToastContent from './SmbOsMountFallbackToastContent.svelte'

function volume(id: string, smbConnectionState: VolumeInfo['smbConnectionState']): VolumeInfo {
  return { id, name: id, path: `/Volumes/${id}`, smbConnectionState } as VolumeInfo
}

beforeEach(async () => {
  addToast.mockClear()
  dismissToast.mockClear()
  await startOsMountNoticeBridge()
})

describe('the OS-mount fallback notice', () => {
  it('names the share and hands the volume to the retry button', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive' })

    expect(addToast).toHaveBeenCalledTimes(1)
    const [content, options] = addToast.mock.calls[0]
    expect(content).toBe(SmbOsMountFallbackToastContent)
    expect(options.props).toEqual({ volumeId: 'smb-archive', share: 'archive' })
  })

  it('stays up until the user acts on it, because the share is slow the whole time', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive' })

    const [, options] = addToast.mock.calls[0]
    expect(options.dismissal).toBe('persistent')
    expect(options.level).toBe('info')
  })

  it('dedups per volume, so a repeat replaces the notice instead of stacking one', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive' })

    const [, options] = addToast.mock.calls[0]
    expect(options.id).toBe(osMountNoticeToastId('smb-archive'))
  })

  it('retires itself once the share reports a direct connection', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive' })

    emitVolumes({ data: [volume('smb-archive', 'direct')], timedOut: false })

    expect(dismissToast).toHaveBeenCalledWith(osMountNoticeToastId('smb-archive'))
  })

  it('leaves the notice up while the share is still on the OS mount', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive' })

    emitVolumes({ data: [volume('smb-archive', 'os_mount')], timedOut: false })

    expect(dismissToast).not.toHaveBeenCalled()
  })

  it('retires only the share that went direct, not every notice on screen', () => {
    emitFallback({ volumeId: 'smb-archive', share: 'archive' })
    emitFallback({ volumeId: 'smb-photos', share: 'photos' })

    emitVolumes({
      data: [volume('smb-archive', 'direct'), volume('smb-photos', 'os_mount')],
      timedOut: false,
    })

    expect(dismissToast).toHaveBeenCalledExactlyOnceWith(osMountNoticeToastId('smb-archive'))
  })

  it('unsubscribes both listeners together', async () => {
    const unlisten = await startOsMountNoticeBridge()

    unlisten()

    expect(unlistenFallback).toHaveBeenCalled()
    expect(unlistenVolumes).toHaveBeenCalled()
  })
})
