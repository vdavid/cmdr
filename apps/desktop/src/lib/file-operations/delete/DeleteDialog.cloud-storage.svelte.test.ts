/**
 * `DeleteDialog.svelte`: the banner a trash routed into a permanent delete shows,
 * and the live flip a folder's scan walk can trigger.
 *
 * Online-only content in a cloud-storage folder can't go to the Trash without
 * being downloaded first, so F8 opens THIS dialog instead of the trash one. The
 * person asked for a trash and got a delete, so the banner has to say why, and
 * the switch that would send them back into that download is gone.
 *
 * A selected FOLDER can't carry the flag itself, so its answer arrives mid-scan.
 * These pin that the dialog flips itself when it lands, and that a confirm
 * pressed before it lands doesn't settle the question by luck.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DeleteDialog from './DeleteDialog.svelte'
import type { ScanPreviewCompleteEvent, ScanPreviewProgressEvent } from '$lib/ipc/bindings'

/** Scan-event listeners the mounted dialog registered, so a test can drive the walk. */
const listeners = {
  progress: [] as ((event: ScanPreviewProgressEvent) => void)[],
  complete: [] as ((event: ScanPreviewCompleteEvent) => void)[],
}

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn((handler: (event: ScanPreviewProgressEvent) => void) => {
    listeners.progress.push(handler)
    return Promise.resolve(() => {})
  }),
  onScanPreviewComplete: vi.fn((handler: (event: ScanPreviewCompleteEvent) => void) => {
    listeners.complete.push(handler)
    return Promise.resolve(() => {})
  }),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

const FOLDER = '/Users/me/Library/CloudStorage/Dropbox/Work'
/** The two banners' opening sentences: the only place they differ in what they claim. */
const ALL_ONLINE_ONLY = 'Everything you selected is online-only.'
const SOME_ONLINE_ONLY = 'Some of your selection is online-only.'
/** The remedy only the mixed wording can offer; with everything evicted it would leave nothing selected. */
const DESELECT_REMEDY = 'deselect all online-only files'

function mountDialog(overrides: Record<string, unknown>): {
  target: HTMLElement
  onConfirm: ReturnType<typeof vi.fn>
} {
  listeners.progress = []
  listeners.complete = []
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onConfirm = vi.fn()
  mount(DeleteDialog, {
    target,
    props: {
      sourceItems: [{ name: 'emclient.pkg', isDirectory: false, isSymlink: false, size: 120 }],
      sourcePaths: [`${FOLDER}/emclient.pkg`],
      sourceFolderPath: FOLDER,
      isPermanent: true,
      supportsTrash: false,
      isFromCursor: true,
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      onConfirm,
      onCancel: () => {},
      ...overrides,
    },
  })
  return { target, onConfirm }
}

/** Lets the dialog's `onMount` finish registering its scan listeners. */
async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await tick()
}

function bannerText(target: HTMLElement): string {
  return target.querySelector('#delete-warning-text')?.textContent ?? ''
}

/** The line that says a press was declined, or `null` while no press has been. */
function handedBackText(target: HTMLElement): string | null {
  return target.querySelector('[data-test="delete-handed-back"]')?.textContent ?? null
}

function confirmButton(target: HTMLElement): HTMLButtonElement {
  const buttons = [...target.querySelectorAll('button')]
  const button = buttons.at(-1)
  if (!button) throw new Error('the dialog rendered no footer buttons')
  return button
}

function completion(onlineOnlyFound: boolean): ScanPreviewCompleteEvent {
  return {
    previewId: 'preview-1',
    filesTotal: 3,
    dirsTotal: 1,
    bytesTotal: 300,
    dedupBytesTotal: 300,
    estimatedCompressedBytes: null,
    onlineOnlyFound,
  }
}

describe('DeleteDialog over online-only cloud content', () => {
  it('explains the swap, and offers no way back to the trash', async () => {
    const { target } = mountDialog({ cloudOnlineOnly: 'all' })
    await tick()

    expect(bannerText(target)).toContain(ALL_ONLINE_ONLY)
    expect(bannerText(target)).toContain('There’ll be no copies in the Trash')
    expect(bannerText(target)).toContain('cloud services usually keep their own trash for about 30 days')
    expect(target.querySelector('[role="switch"]')).toBeNull()
  })

  /** The one remedy that stops working when everything is evicted: deselecting the
   *  online-only items would leave nothing selected, so only the mixed copy says it. */
  it('offers deselecting only when part of the selection is still ordinary', async () => {
    const mixed = mountDialog({ cloudOnlineOnly: 'mixed' })
    await tick()
    expect(bannerText(mixed.target)).toContain(SOME_ONLINE_ONLY)
    expect(bannerText(mixed.target)).toContain(DESELECT_REMEDY)

    const all = mountDialog({ cloudOnlineOnly: 'all' })
    await tick()
    expect(bannerText(all.target)).not.toContain(DESELECT_REMEDY)
    expect(bannerText(all.target)).toContain('make these files available offline first')
  })

  /** The generic banner is about a volume with no trash (FAT32, SMB); a cloud
   *  folder is on the boot volume, so the two must not be confused. */
  it('leaves the generic no-trash banner to the volumes it describes', async () => {
    const { target } = mountDialog({})
    await tick()

    expect(bannerText(target)).toContain('This volume doesn’t support trash.')
    expect(bannerText(target)).not.toContain('online-only')
  })

  /** A selected folder opens as an ordinary trash. The walk is the only thing
   *  that can see an evicted file inside it, so the dialog has to flip itself
   *  when one turns up, before anything is pressed. */
  it('flips to the permanent delete when the walk finds an online-only file', async () => {
    const { target } = mountDialog({
      sourceItems: [{ name: 'Work', isDirectory: true, isSymlink: false }],
      sourcePaths: [FOLDER],
      isPermanent: false,
      supportsTrash: true,
      cloudFolderMayHoldOnlineOnly: true,
    })
    await settle()

    expect(bannerText(target)).toBe('')
    expect(target.querySelector('[role="switch"]')).not.toBeNull()

    for (const handler of listeners.complete) handler(completion(true))
    await tick()

    // The walk reports one boolean for the whole folder, so all it can honestly
    // support is the mixed wording: the folder almost certainly holds ordinary
    // files beside the evicted one it just tripped over.
    expect(bannerText(target)).toContain(SOME_ONLINE_ONLY)
    expect(bannerText(target)).not.toContain(ALL_ONLINE_ONLY)
    expect(target.querySelector('[role="switch"]')).toBeNull()
  })

  /** A fully materialized folder stays exactly as it is today. */
  it('stays a trash when the walk finds nothing evicted', async () => {
    const { target, onConfirm } = mountDialog({
      sourceItems: [{ name: 'Work', isDirectory: true, isSymlink: false }],
      sourcePaths: [FOLDER],
      isPermanent: false,
      supportsTrash: true,
      cloudFolderMayHoldOnlineOnly: true,
    })
    await settle()

    for (const handler of listeners.complete) handler(completion(false))
    await tick()

    expect(bannerText(target)).toBe('')
    confirmButton(target).click()
    await settle()
    expect(onConfirm).toHaveBeenCalledWith('preview-1', false)
  })

  /** The race the whole wait exists for. Pressing "Move to trash" while the walk
   *  is still counting must not run a trash the answer is about to rule out. */
  it('holds a confirm until the walk answers, then hands the dialog back when it says online-only', async () => {
    const { target, onConfirm } = mountDialog({
      sourceItems: [{ name: 'Work', isDirectory: true, isSymlink: false }],
      sourcePaths: [FOLDER],
      isPermanent: false,
      supportsTrash: true,
      cloudFolderMayHoldOnlineOnly: true,
    })
    await settle()

    confirmButton(target).click()
    await settle()
    expect(onConfirm).not.toHaveBeenCalled()
    expect(handedBackText(target), 'nothing to explain yet: the press is still waiting').toBeNull()

    // One hit is the whole answer, so a progress tick releases the wait: a folder
    // holding 50 GB of evicted content needn't finish counting first.
    for (const handler of listeners.progress) {
      handler({
        previewId: 'preview-1',
        filesFound: 1,
        dirsFound: 0,
        bytesFound: 1,
        currentPath: null,
        currentDir: null,
        expectedFilesTotal: null,
        expectedBytesTotal: null,
        onlineOnlyFound: true,
      })
    }
    await settle()

    expect(onConfirm).not.toHaveBeenCalled()
    expect(bannerText(target)).toContain(SOME_ONLINE_ONLY)

    // The dialog stays open and the button's label changed under the person's
    // finger, so without this line nothing tells them their press didn't take.
    expect(handedBackText(target)).toContain('so the button below now offers Delete')

    // And the second press, now over a dialog that says what it will do, goes.
    confirmButton(target).click()
    await settle()
    expect(onConfirm).toHaveBeenCalledWith('preview-1', true)
    expect(handedBackText(target), 'the explanation goes with the press it explained').toBeNull()
  })
})
