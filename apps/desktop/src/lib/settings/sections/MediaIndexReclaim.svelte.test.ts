/**
 * Tests for `MediaIndexReclaim.svelte`: the reclaim line only appears once the count
 * settles and the leftover is meaningfully large, and clicking through confirms, prunes,
 * and toasts. The pure visibility rule lives in `media-index-reclaim.ts` (tested there);
 * this pins the component wiring around it. Tauri, confirm, and toast are mocked so the
 * test runs with no runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { previewMock, pruneMock, confirmMock, addToastMock } = vi.hoisted(() => ({
  previewMock: vi.fn(),
  pruneMock: vi.fn(),
  confirmMock: vi.fn(),
  addToastMock: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexReclaimPreview: previewMock,
  mediaIndexPruneBelowThreshold: pruneMock,
}))
vi.mock('$lib/media-index/enabled-volumes', () => ({ getEnabledMediaIndexVolumeIds: () => ['root'] }))
vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: confirmMock }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastMock }))
vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => ({ warn: vi.fn(), info: vi.fn() }) }))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/intl/number-format', () => ({ formatInteger: (n: number) => String(n) }))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({ formatFileSize: (b: number) => `${String(b)}B` }))

import MediaIndexReclaim from './MediaIndexReclaim.svelte'

const LARGE_LEFTOVER = {
  totalStored: 200_000,
  coveredStored: 150,
  doomedCount: 199_850,
  estimatedBytes: 2_000_000_000,
  pending: false,
}

async function flush(): Promise<void> {
  // Let the mount effect's async fetch resolve and the DOM re-render.
  await tick()
  await Promise.resolve()
  await Promise.resolve()
  await tick()
}

async function mountReclaim(props: { threshold: number; blocked: boolean }): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexReclaim, { target, props })
  await flush()
  return target
}

beforeEach(() => {
  previewMock.mockReset()
  pruneMock.mockReset()
  confirmMock.mockReset()
  addToastMock.mockReset()
})

describe('MediaIndexReclaim', () => {
  it('shows the reclaim line and button when the leftover is large and settled', async () => {
    previewMock.mockResolvedValue(LARGE_LEFTOVER)
    const target = await mountReclaim({ threshold: 0.0, blocked: false })

    expect(previewMock).toHaveBeenCalledWith(0.0, ['root'])
    expect(target.querySelector('.mi-reclaim')).not.toBeNull()
    expect(target.querySelector('button')).not.toBeNull()
    target.remove()
  })

  it('stays hidden when blocked (waiting on importance / a scan)', async () => {
    previewMock.mockResolvedValue(LARGE_LEFTOVER)
    const target = await mountReclaim({ threshold: 0.0, blocked: true })
    // Blocked never even queries.
    expect(previewMock).not.toHaveBeenCalled()
    expect(target.querySelector('.mi-reclaim')).toBeNull()
    target.remove()
  })

  it('stays hidden when the backend reports pending', async () => {
    previewMock.mockResolvedValue({ ...LARGE_LEFTOVER, pending: true })
    const target = await mountReclaim({ threshold: 0.0, blocked: false })
    expect(target.querySelector('.mi-reclaim')).toBeNull()
    target.remove()
  })

  it('stays hidden when the leftover is too small to bother', async () => {
    previewMock.mockResolvedValue({ totalStored: 200_000, coveredStored: 199_950, doomedCount: 50, estimatedBytes: 1, pending: false })
    const target = await mountReclaim({ threshold: 0.0, blocked: false })
    expect(target.querySelector('.mi-reclaim')).toBeNull()
    target.remove()
  })

  it('confirms, prunes, and toasts the freed space on click', async () => {
    previewMock.mockResolvedValue(LARGE_LEFTOVER)
    confirmMock.mockResolvedValue(true)
    pruneMock.mockResolvedValue({ deletedRows: 199_850, freedBytes: 2_000_000_000 })
    const target = await mountReclaim({ threshold: 0.2, blocked: false })

    const button = target.querySelector('button')
    if (!button) throw new Error('reclaim button not found')
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flush()

    expect(confirmMock).toHaveBeenCalledOnce()
    expect(pruneMock).toHaveBeenCalledWith(0.2, ['root'])
    expect(addToastMock).toHaveBeenCalledWith('settings.mediaIndex.reclaim.freed', { level: 'success' })
    target.remove()
  })

  it('does not prune when the confirm dialog is cancelled', async () => {
    previewMock.mockResolvedValue(LARGE_LEFTOVER)
    confirmMock.mockResolvedValue(false)
    const target = await mountReclaim({ threshold: 0.0, blocked: false })

    const button = target.querySelector('button')
    if (!button) throw new Error('reclaim button not found')
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flush()

    expect(confirmMock).toHaveBeenCalledOnce()
    expect(pruneMock).not.toHaveBeenCalled()
    target.remove()
  })
})
