/**
 * `DeleteDialog.svelte` closed before its scan preview is under way.
 *
 * The dialog starts its scan preview on mount, and that start is async: four listener
 * registrations, then the `startScanPreview` IPC. A quick Escape, or an MCP close, unmounts
 * the dialog partway through. Both parents (`DialogManager`, `DialogGallery`) null their
 * props object on close, and a Svelte prop is a live getter into it, so a prop read after
 * unmount throws: the `null is not an object (evaluating 't.deleteDialogProps.sourcePaths')`
 * rejection E2E runs kept logging. The props here are built the same way. Past the throw,
 * nothing the start leaves behind may outlive the dialog either: a listener, or a preview
 * whose id only lands after teardown ran.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import * as commands from '$lib/tauri-commands'
import type { DeleteDialogPropsData } from '$lib/file-explorer/pane/dialog-props'
import DeleteDialog from './DeleteDialog.svelte'

vi.mock('$lib/tauri-commands', () => ({
  estimateOperationCost: vi.fn(() => Promise.resolve([])),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  startScanPreview: vi.fn(),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  onScanPreviewProgress: vi.fn(),
  onScanPreviewComplete: vi.fn(),
  onScanPreviewError: vi.fn(),
  onScanPreviewCancelled: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number | undefined) => (n === undefined ? '' : `${String(n)} B`)),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

/** What `DialogManager` hands the dialog out of its `deleteDialogProps` slot. */
type DialogData = Pick<
  DeleteDialogPropsData,
  | 'sourceItems'
  | 'sourcePaths'
  | 'sourceFolderPath'
  | 'isPermanent'
  | 'supportsTrash'
  | 'isFromCursor'
  | 'sortColumn'
  | 'sortOrder'
  | 'sourceVolumeId'
>
type DialogProps = DialogData & {
  onConfirm: (previewId: string | null, isPermanent: boolean) => void
  onCancel: () => void
}

const dialogData: DialogData = {
  sourceItems: [{ name: 'a.txt', isDirectory: false, isSymlink: false, size: 12 }],
  sourcePaths: ['/src/a.txt'],
  sourceFolderPath: '/src',
  isPermanent: false,
  supportsTrash: true,
  isFromCursor: true,
  sortColumn: 'name',
  sortOrder: 'ascending',
  sourceVolumeId: 'root',
}

interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
}

function deferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((settle) => {
    resolve = settle
  })
  return { promise, resolve }
}

/** The parent's props slot: a dialog is open while it holds data, and closing nulls it. */
const parent: { data: DialogData | null } = { data: null }

/** What a parent's compiled prop getter does once its props object is gone. */
function fromParent<K extends keyof DialogData>(key: K): DialogData[K] {
  if (parent.data === null) throw new TypeError(`null is not an object (evaluating 'deleteDialogProps.${key}')`)
  return parent.data[key]
}

/** Props shaped like `DialogManager`'s: every value is a live getter into the parent's slot. */
const propsFromParent: DialogProps = {
  get sourceItems() {
    return fromParent('sourceItems')
  },
  get sourcePaths() {
    return fromParent('sourcePaths')
  },
  get sourceFolderPath() {
    return fromParent('sourceFolderPath')
  },
  get isPermanent() {
    return fromParent('isPermanent')
  },
  get supportsTrash() {
    return fromParent('supportsTrash')
  },
  get isFromCursor() {
    return fromParent('isFromCursor')
  },
  get sortColumn() {
    return fromParent('sortColumn')
  },
  get sortOrder() {
    return fromParent('sortOrder')
  },
  get sourceVolumeId() {
    return fromParent('sourceVolumeId')
  },
  onConfirm: () => {},
  onCancel: () => {},
}

/** Lets every pending continuation run, and Node report any rejection nobody handled. */
async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await new Promise((done) => setTimeout(done, 0))
  }
}

describe('DeleteDialog closed before its scan preview is under way', () => {
  let registrations: Deferred<undefined>
  let unlistens: Array<() => void>
  let rejections: unknown[]
  const onRejection = (reason: unknown): void => {
    rejections.push(reason)
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
    parent.data = dialogData
    registrations = deferred()
    unlistens = []
    rejections = []
    process.on('unhandledRejection', onRejection)
    for (const register of [
      commands.onScanPreviewProgress,
      commands.onScanPreviewComplete,
      commands.onScanPreviewError,
      commands.onScanPreviewCancelled,
    ]) {
      vi.mocked(register).mockImplementation(async () => {
        await registrations.promise
        const unlisten = vi.fn()
        unlistens.push(unlisten)
        return unlisten
      })
    }
  })

  afterEach(() => {
    process.off('unhandledRejection', onRejection)
  })

  /** Mounts the dialog, returning what closes it the way its parent does: null the slot, then unmount. */
  async function open(): Promise<() => void> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(DeleteDialog, { target, props: propsFromParent })
    await tick()
    return () => {
      parent.data = null
      void unmount(component)
    }
  }

  it('reads no props and keeps no listener when it closes while its listeners are still registering', async () => {
    const close = await open()

    close()
    registrations.resolve(undefined)
    await settle()

    expect(rejections).toEqual([])
    expect(commands.startScanPreview).not.toHaveBeenCalled()
    expect(unlistens.length).toBeGreaterThan(0)
    for (const unlisten of unlistens) expect(unlisten).toHaveBeenCalledOnce()
  })

  it('frees a preview whose id only lands after the dialog closed', async () => {
    const preview = deferred<{ previewId: string }>()
    vi.mocked(commands.startScanPreview).mockReturnValue(preview.promise)
    registrations.resolve(undefined)
    const close = await open()
    await settle()
    expect(commands.startScanPreview).toHaveBeenCalledWith(['/src/a.txt'], 'name', 'ascending', 500, 'root')

    close()
    preview.resolve({ previewId: 'preview-late' })
    await settle()

    expect(rejections).toEqual([])
    expect(commands.cancelScanPreview).toHaveBeenCalledWith('preview-late')
  })
})
