/**
 * What `TransferDialog` shows when it CAN'T find something out.
 *
 * Both of its pre-confirm questions ("how big is this?" and "what's already at
 * the destination?") reach a volume that can stop answering, and both used to
 * fail silently: the size scan dropped its spinner and left the tallies at zero,
 * and the conflict check reported "checked, nothing found" for a check that
 * never ran. A dialog that can't tell the user which of those happened is one
 * they can't act on, and the conflict one shades into a lie, because "no
 * conflicts" is what the same UI says when the destination is genuinely empty.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import * as commands from '$lib/tauri-commands'

const startScanPreviewMock = vi.mocked(commands.startScanPreview)

/** The captured `scan-preview-error` listener, so a test can fire the event. */
let scanErrorCb: ((e: { previewId: string; message: string; timedOut: boolean }) => void) | null = null

const scanVolumeForConflictsMock = vi.fn<() => Promise<unknown[]>>(() => Promise.resolve([]))

vi.mock('@tauri-apps/api/path', () => ({
  homeDir: () => Promise.resolve('/Users/test'),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() =>
    Promise.resolve({ data: { totalBytes: 1024 * 1024 * 1024, availableBytes: 1024 * 1024 * 500 } }),
  ),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn((cb: (e: { previewId: string; message: string; timedOut: boolean }) => void) => {
    scanErrorCb = cb
    return Promise.resolve(() => {
      scanErrorCb = null
    })
  }),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: () => scanVolumeForConflictsMock(),
  pathExistsChecked: vi.fn(() => Promise.resolve({ data: true, timedOut: false })),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
  setSetting: vi.fn(),
  getDefaultValue: vi.fn(() => 6),
  onSpecificSettingChange: vi.fn(() => () => {}),
  getSettingDefinition: vi.fn(() => ({ label: '', constraints: {} })),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'nas', name: 'NAS share', path: '/Volumes/nas', category: 'attached_volume', isEjectable: false },
  ],
}))

async function flushMicrotasks(rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

function mountDialog(onConfirm: (...args: unknown[]) => void = () => {}): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Volumes/nas/footage'],
      destinationPath: '/Users/test/dest',
      currentVolumeId: 'root',
      fileCount: 0,
      folderCount: 1,
      sourceFolderPath: '/Volumes/nas',
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'nas',
      destVolumeId: 'root',
      autoConfirm: false,
      onConfirm,
      onCancel: () => {},
    },
  })
  return target
}

beforeEach(() => {
  vi.clearAllMocks()
  scanErrorCb = null
  scanVolumeForConflictsMock.mockResolvedValue([])
  startScanPreviewMock.mockResolvedValue({ previewId: 'preview-1' })
  document.body.innerHTML = ''
})

describe('a size scan that gives up', () => {
  it('says the source stopped responding, and offers a retry', async () => {
    const target = mountDialog()
    await flushMicrotasks()

    scanErrorCb?.({ previewId: 'preview-1', message: 'the share stopped responding', timedOut: true })
    await flushMicrotasks()

    const stats = target.querySelector('.scan-stats')
    expect(stats?.getAttribute('data-scan-state')).toBe('unavailable')
    // The spinner is a claim that counting is still happening; it must be gone.
    expect(stats?.querySelector('.scan-status')).toBeNull()

    const notice = target.querySelector('.scan-unavailable')
    expect(notice?.textContent).toContain("isn't responding")

    const retry = notice?.querySelector('button')
    expect(retry).not.toBeNull()
    startScanPreviewMock.mockClear()
    retry?.click()
    await flushMicrotasks()
    expect(startScanPreviewMock).toHaveBeenCalledTimes(1)
  })

  it('keeps the honest state for a scan that stopped for any other reason', async () => {
    const target = mountDialog()
    await flushMicrotasks()

    scanErrorCb?.({ previewId: 'preview-1', message: 'a folder went missing', timedOut: false })
    await flushMicrotasks()

    const stats = target.querySelector('.scan-stats')
    expect(stats?.getAttribute('data-scan-state')).toBe('unavailable')
    // No "not responding" claim for something that answered and said no.
    expect(target.querySelector('.scan-unavailable')?.textContent).not.toContain("isn't responding")
  })
})

describe('a conflict check that gives up', () => {
  it('says it could not check, rather than showing the no-conflicts UI', async () => {
    scanVolumeForConflictsMock.mockRejectedValue(new Error('Operation timed out'))
    const target = mountDialog()
    await flushMicrotasks()

    const body = target.querySelector('.dialog-body')
    expect(body?.getAttribute('data-conflict-state')).toBe('unknown')
    expect(target.querySelector('.conflicts-checking')).toBeNull()
    expect(target.querySelector('.conflicts-unknown')?.textContent).toContain("couldn't check")
  })

  it('still lets the transfer start, since the backend asks about each clash it meets', async () => {
    scanVolumeForConflictsMock.mockRejectedValue(new Error('Operation timed out'))
    const confirmed = vi.fn()
    const target = mountDialog(confirmed)
    await flushMicrotasks()

    target.querySelector<HTMLButtonElement>('.btn-primary')?.click()
    await flushMicrotasks()

    expect(confirmed).toHaveBeenCalledTimes(1)
    // An unknown check contributes NO names: the bulk pre-skip list is a perf
    // hint, and inventing entries for it would skip files nobody checked.
    expect(confirmed.mock.calls[0]?.[5]).toEqual([])
    // And the policy it dispatches with is still "ask me about each one".
    expect(confirmed.mock.calls[0]?.[3]).toBe('stop')
  })
})
