/** Complete Copy/Move targets and the rename-by-move confirmation. */
import { describe, it, expect, vi } from 'vitest'
import { tick } from 'svelte'
import type { TransferConfirmPayload } from '$lib/file-explorer/pane/dialog-props'
import {
  flushMicrotasks,
  makeConflict,
  mountDialog,
  confirmButton,
  pathInput,
  scanState,
  startScanPreviewMock,
  scanVolumeForConflictsMock,
  type ConfirmFn,
} from './test-transfer-dialog-harness'

describe('copy to a filename', () => {
  it.each(['copy', 'move'] as const)(
    'confirms a relative %s target beside the source on another volume',
    async (operationType) => {
      const onConfirm = vi.fn<ConfirmFn>()
      const target = mountDialog({
        operationType,
        sourcePaths: ['/Users/test/notes.txt'],
        currentVolumeId: 'ext',
        onConfirm,
      })
      await flushMicrotasks()
      const input = target.querySelector<HTMLInputElement>('input[type="text"]')
      if (!input) throw new Error('path input not rendered')
      input.value = 'notes backup.txt'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      await tick()
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await flushMicrotasks()
      expect(onConfirm).toHaveBeenCalledWith(
        expect.objectContaining({
          destination: '/Users/test',
          destinationName: 'notes backup.txt',
          volumeId: 'root',
          operationType,
        }),
      )
      expect(target.querySelector('#transfer-path-error')).toBeNull()
    },
  )
})

describe('complete single-copy target', () => {
  it.each(['copy', 'move'] as const)('shows the original name in the %s target', async (operationType) => {
    const target = mountDialog({ operationType, sourcePaths: ['/Users/test/notes.txt'] })
    await flushMicrotasks()
    expect(pathInput(target).value).toBe('/Users/test/dest/notes.txt')
  })

  it.each(['copy', 'move'] as const)('accepts full and relative %s filenames', async (operationType) => {
    for (const [entered, destination, destinationName, volumeId] of [
      ['/tmp/new.txt', '/tmp', 'new.txt', 'root'],
      ['copies/new.txt', '/Users/test/copies', 'new.txt', 'root'],
      ['../new.txt', '/Users', 'new.txt', 'root'],
      ['/Volumes/External/copies/new.txt', '/copies', 'new.txt', 'ext'],
      ['~/new.txt', '/Users/test', 'new.txt', 'root'],
    ]) {
      const onConfirm = vi.fn<ConfirmFn>()
      const target = mountDialog({ operationType, sourcePaths: ['/Users/test/notes.txt'], onConfirm })
      await flushMicrotasks()
      const input = pathInput(target)
      input.value = entered
      input.dispatchEvent(new Event('input', { bubbles: true }))
      await tick()
      confirmButton(target).click()
      await flushMicrotasks()
      expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ destination, destinationName, volumeId }))
    }
  })

  it.each([
    ['/tmp/', '/tmp'],
    ['../', '/Users'],
    ['copies/', '/Users/test/copies'],
  ])('reads a trailing slash in %s as "into this folder", keeping the name', async (entered, destination) => {
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({ sourcePaths: ['/Users/test/notes.txt'], onConfirm })
    await flushMicrotasks()
    const input = pathInput(target)
    input.value = entered
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ destination, destinationName: 'notes.txt' }))
    expect(target.querySelector('#transfer-path-error')).toBeNull()
  })
})

describe('a target path that names an existing folder', () => {
  // Pasting a folder path keeps "the last segment is the new name", so the
  // dialog says how to put the item inside instead.
  async function mountWithFolderAtTarget(entered: string, sourcePath: string): Promise<HTMLDivElement> {
    scanVolumeForConflictsMock.mockResolvedValue([
      makeConflict({ sourcePath: 'Documents', sourceIsDirectory: true, destIsDirectory: true }),
    ])
    const target = mountDialog({ sourcePaths: [sourcePath] })
    await flushMicrotasks()
    const input = pathInput(target)
    input.value = entered
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    return target
  }

  it("hints at the trailing slash when the item would take the folder's name", async () => {
    const target = await mountWithFolderAtTarget('/Users/test/Documents', '/Users/test/src/Photos')
    await vi.waitFor(() => {
      expect(target.querySelector('.named-folder-hint')?.textContent).toContain('/')
    })
    expect(target.querySelector('.named-folder-hint')?.textContent).toContain('Documents')
    expect(target.querySelector('.named-folder-hint')?.textContent).toContain('Photos')
  })

  it('stays quiet when the item keeps its own name, an ordinary merge', async () => {
    const target = await mountWithFolderAtTarget('/Users/test/Documents', '/Users/test/src/Documents')
    await vi.waitFor(() => {
      expect(target.querySelector('.merge-info')).not.toBeNull()
    })
    expect(target.querySelector('.named-folder-hint')).toBeNull()
  })
})

describe('named copies on a phone', () => {
  it.each([
    ['/DCIM/new.jpg', '/DCIM'],
    ['backups/new.jpg', '/DCIM/backups'],
  ])('resolves %s in the phone namespace', async (entered, destination) => {
    const onConfirm = vi.fn<ConfirmFn>()
    const target = mountDialog({
      sourcePaths: ['/mtp-20-5/65538/DCIM/original.jpg'],
      sourceFolderPath: '/mtp-20-5/65538/DCIM',
      sourceVolumeId: 'mtp-336592896:65538',
      currentVolumeId: 'mtp-336592896:65538',
      destinationPath: '/mtp-20-5/65538/DCIM',
      onConfirm,
    })
    await flushMicrotasks()
    const input = pathInput(target)
    input.value = entered
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({
        destination,
        destinationName: 'new.jpg',
        volumeId: 'mtp-336592896:65538',
      }),
    )
  })
})

it('resolves a relative copy beside the selected source when it is outside the pane folder', async () => {
  const onConfirm = vi.fn<ConfirmFn>()
  const target = mountDialog({ sourcePaths: ['/Users/other/original.txt'], sourceFolderPath: '/Users/test', onConfirm })
  await flushMicrotasks()
  const input = pathInput(target)
  input.value = 'backup.txt'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  confirmButton(target).click()
  await flushMicrotasks()
  expect(onConfirm).toHaveBeenCalledWith(
    expect.objectContaining({ destination: '/Users/other', destinationName: 'backup.txt' }),
  )
})

it.each(['copy', 'move'] as const)('warns and disables %s for the identical source target', async (operationType) => {
  const onConfirm = vi.fn<ConfirmFn>()
  const target = mountDialog({ operationType, sourcePaths: ['/Users/test/notes.txt'], onConfirm })
  await flushMicrotasks()
  const input = pathInput(target)
  for (const entered of ['/Users/test/notes.txt', 'notes.txt', './copies/../notes.txt']) {
    input.value = entered
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(target.querySelector('#transfer-path-error')?.textContent).toBe('“notes.txt” is already in this location')
    expect(confirmButton(target).disabled).toBe(true)
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flushMicrotasks()
    expect(onConfirm).not.toHaveBeenCalled()
  }
  input.value = 'backup.txt'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  expect(confirmButton(target).disabled).toBe(false)
})

describe('TransferDialog rename mode (a rename that copies, confirmed as a move)', () => {
  // A same-volume move on a non-default volume: exactly what the fast path would
  // otherwise grab, so each test also proves rename mode stays off it.
  const RENAME = {
    operationType: 'move' as const,
    sourceVolumeId: 'ext',
    currentVolumeId: 'ext',
    sourcePaths: ['/Volumes/External/bucket/photos'],
    sourceFolderPath: '/Volumes/External/bucket',
    destinationPath: '/Volumes/External/bucket',
    newName: 'pictures',
  }

  function pathInput(target: HTMLElement): HTMLInputElement {
    const input = target.querySelector<HTMLInputElement>('input[aria-label="Destination path"]')
    if (!input) throw new Error('path input not rendered')
    return input
  }

  async function typePath(target: HTMLElement, value: string): Promise<void> {
    const input = pathInput(target)
    input.value = value
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await flushMicrotasks()
  }

  it('prefills the path box with the folder plus the new name', async () => {
    const target = mountDialog(RENAME)
    await flushMicrotasks()

    expect(pathInput(target).value).toBe('/bucket/pictures')
  })

  it('hides the Copy/Move/Compress toggle and says why it runs as a move', async () => {
    const target = mountDialog(RENAME)
    await flushMicrotasks()

    expect(target.querySelector('.tg-root')).toBeNull()
    expect(target.querySelector('.rename-hint')?.textContent).toContain('pause or cancel')
  })

  it('runs the deep scan, so the dialog shows what the rename copies', async () => {
    const target = mountDialog(RENAME)
    await flushMicrotasks()

    expect(startScanPreviewMock).toHaveBeenCalledTimes(1)
    expect(scanState(target)).toBe('counting')
  })

  it('skips the top-level conflict check, where the source would clash with itself', async () => {
    const target = mountDialog(RENAME)
    await flushMicrotasks()

    expect(scanVolumeForConflictsMock).not.toHaveBeenCalled()
    expect(target.querySelector('.dialog-body')?.getAttribute('data-conflict-state')).toBe('skipped')
  })

  it('confirms with the folder as the destination and the leaf as the new name', async () => {
    let captured: TransferConfirmPayload | null = null
    const target = mountDialog({ ...RENAME, onConfirm: (p) => (captured = p) })
    await flushMicrotasks()
    await typePath(target, '/bucket/archive/pictures 2024')

    confirmButton(target).click()
    await flushMicrotasks()

    expect(captured).toMatchObject({
      destination: '/bucket/archive',
      newName: 'pictures 2024',
      operationType: 'move',
      previewId: 'preview-1',
    })
  })

  it('refuses an empty new name, and lets the source folder stand as the destination', async () => {
    const target = mountDialog(RENAME)
    await flushMicrotasks()
    expect(target.querySelector('.path-error')).toBeNull()

    await typePath(target, '/bucket/')
    expect(target.querySelector('.path-error')?.textContent).toBeTruthy()
    expect(confirmButton(target).disabled).toBe(true)
  })

  it('refuses a new name with a disallowed character', async () => {
    const target = mountDialog(RENAME)
    await flushMicrotasks()

    await typePath(target, '/bucket/bad\u0000name')
    expect(target.querySelector('.path-error')?.textContent).toBeTruthy()
  })
})
