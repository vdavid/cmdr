/**
 * Tier 3 a11y tests for the transfer dialogs: the destination picker, the
 * progress dialog, the conflict resolver, the error dialog, and the archive
 * password prompt.
 *
 * One file per dialog would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"), and these dialogs pull the directory's heaviest import graph. Each
 * block below keeps its dialog's own doc comment, props, and assertions.
 *
 * The mock surface is mutable where the source files disagreed: `getSetting`
 * answered 500 in two of them, and the volume store listed one drive in one and
 * two in another. `null` means "use the real export", which is what the blocks
 * that never stubbed a module had.
 *
 * The presentational pieces these dialogs compose live in
 * `transfer-parts.a11y.test.ts`; one merged file for the whole directory would
 * clear the 800-line `file-length` mark.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { VolumeConflictInfo, WriteConflictEvent } from '$lib/tauri-commands'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'

const stubs = vi.hoisted(() => ({
  getSetting: null as ((id: string) => unknown) | null,
  volumes: null as (() => unknown[]) | null,
  // Mutable per-test result for the conflict scan, so `TransferDialog`'s
  // merge-info and cross-type-warning cases can drive specific collision shapes.
  conflictResult: [] as unknown[],
  formatFileSize: null as ((n: number) => string) | null,
}))

// The union of what these five dialogs reach for, over the real module: each
// source file stubbed a different slice, so a bare union would hand a dialog a
// missing export it never had.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
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
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: vi.fn(() => Promise.resolve(stubs.conflictResult)),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  listOperations: vi.fn(() => Promise.resolve([])),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realGetSetting = actual.getSetting as (id: string) => unknown
  return {
    ...actual,
    getSetting: (id: string) => (stubs.getSetting ? stubs.getSetting(id) : realGetSetting(id)),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realFormat = actual.formatFileSize as (n: number) => string
  return {
    ...actual,
    formatFileSize: (n: number) => (stubs.formatFileSize ? stubs.formatFileSize(n) : realFormat(n)),
    getFileSizeFormat: vi.fn(() => 'binary'),
    getFileSizeUnit: vi.fn(() => 'bytes'),
  }
})

vi.mock('$lib/stores/volume-store.svelte', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realVolumes = actual.getVolumes as () => unknown[]
  return {
    ...actual,
    getVolumes: () => (stubs.volumes ? stubs.volumes() : realVolumes()),
  }
})

import ArchivePasswordDialog from './ArchivePasswordDialog.svelte'
import TransferConflictDialog from './TransferConflictDialog.svelte'
import TransferDialog from './TransferDialog.svelte'
import TransferErrorDialog from './TransferErrorDialog.svelte'
import TransferProgressDialog from './TransferProgressDialog.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

beforeEach(() => {
  stubs.getSetting = null
  stubs.volumes = null
  stubs.formatFileSize = null
  stubs.conflictResult = []
})

/**
 * Tier 3 a11y tests for `ArchivePasswordDialog.svelte`.
 *
 * Covers the first prompt and the wrong-attempt re-prompt. Tauri IPC is stubbed
 * so the dialog can mount cleanly in happy-dom.
 */
describe('ArchivePasswordDialog a11y', () => {
  it('first prompt has no a11y violations', async () => {
    const target = container()
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit: () => {}, onCancel: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('wrong-attempt re-prompt has no a11y violations', async () => {
    const target = container()
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: true, onSubmit: () => {}, onCancel: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `TransferDialog.svelte`.
 *
 * Copy/move destination picker. Volume store, Tauri IPC, and settings
 * are stubbed. Tests cover the copy and move initial states; the
 * copy/move toggle is always present, so both tests exercise it. The
 * dialog mounts lots of event-listener boilerplate, so events return
 * no-op unsubscribers.
 */
describe('TransferDialog a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 500
    stubs.volumes = (): VolumeInfo[] => [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      { id: 'ext', name: 'External', path: '/Volumes/External', category: 'attached_volume', isEjectable: true },
    ]
  })

  it('copy dialog has no a11y violations', async () => {
    const target = container()
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/file.txt'],
        destinationPath: '/Users/test/dest',
        currentVolumeId: 'root',
        fileCount: 1,
        folderCount: 0,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('move dialog has no a11y violations', async () => {
    const target = container()
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'move',
        sourcePaths: ['/Users/test/file1.txt', '/Users/test/file2.txt'],
        destinationPath: '/Users/test/dest',
        currentVolumeId: 'root',
        fileCount: 2,
        folderCount: 0,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('dialog with a folder-merge info line has no a11y violations', async () => {
    stubs.conflictResult = [
      {
        sourcePath: 'photos',
        destPath: 'photos',
        sourceSize: 0,
        destSize: 0,
        sourceModified: null,
        destModified: null,
        sourceIsDirectory: true,
        destIsDirectory: true,
      } satisfies VolumeConflictInfo,
    ]
    const target = container()
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/photos'],
        destinationPath: '/Users/test/dest',
        currentVolumeId: 'root',
        fileCount: 0,
        folderCount: 1,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    // Let the parallel conflict check resolve so the merge line renders.
    for (let i = 0; i < 6; i++) {
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
      await tick()
    }
    await expectNoA11yViolations(target)
    stubs.conflictResult = []
  })

  it('dialog with the cross-type Overwrite-all warning has no a11y violations', async () => {
    stubs.conflictResult = [
      {
        sourcePath: 'photos',
        destPath: 'photos',
        sourceSize: 0,
        destSize: 0,
        sourceModified: null,
        destModified: null,
        sourceIsDirectory: false,
        destIsDirectory: true,
      } satisfies VolumeConflictInfo,
    ]
    const target = container()
    mount(TransferDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/photos'],
        destinationPath: '/Users/test/dest',
        currentVolumeId: 'root',
        fileCount: 1,
        folderCount: 0,
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        onConfirm: () => {},
        onCancel: () => {},
      },
    })
    for (let i = 0; i < 6; i++) {
      await new Promise<void>((resolve) => setTimeout(resolve, 0))
      await tick()
    }
    // Select Overwrite all to surface the red warning, then assert clean a11y.
    const overwrite = target.querySelector<HTMLInputElement>('input[type="radio"][value="overwrite"]')
    overwrite?.click()
    await tick()
    await expectNoA11yViolations(target)
    stubs.conflictResult = []
  })
})

/**
 * Tier 3 a11y tests for `TransferProgressDialog.svelte`.
 *
 * Progress dialog shown while a copy/move/delete/trash is running. Tests
 * render the default "just-mounted" state for each operation type. The
 * dialog's reactive state updates via event callbacks; our mocks return
 * no-op unsubscribers so only the initial render is audited.
 */
describe('TransferProgressDialog a11y', () => {
  beforeEach(async () => {
    stubs.getSetting = () => 500
    stubs.formatFileSize = (n: number) => `${String(n)} B`
    stubs.volumes = (): VolumeInfo[] => [
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    ]
    // A view needs its window's session registry, the same as in the app.
    await initOperationSessions()
  })

  afterEach(() => {
    destroyOperationSessions()
  })

  it('copy operation (initial "Scanning" state) has no a11y violations', async () => {
    const target = container()
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'copy',
        sourcePaths: ['/Users/test/file.txt'],
        sourceFolderPath: '/Users/test',
        destinationPath: '/Users/test/dest',
        direction: 'right',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        conflictResolution: 'stop',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('move operation has no a11y violations', async () => {
    const target = container()
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'move',
        sourcePaths: ['/Users/test/file1.txt', '/Users/test/file2.txt'],
        sourceFolderPath: '/Users/test',
        destinationPath: '/Users/test/dest',
        direction: 'right',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        destVolumeId: 'root',
        conflictResolution: 'stop',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('delete operation (no destination) has no a11y violations', async () => {
    const target = container()
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'delete',
        sourcePaths: ['/Users/test/file.txt'],
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('trash operation has no a11y violations', async () => {
    const target = container()
    mount(TransferProgressDialog, {
      target,
      props: {
        operationType: 'trash',
        sourcePaths: ['/Users/test/file.txt'],
        sourceFolderPath: '/Users/test',
        sortColumn: 'name',
        sortOrder: 'ascending',
        previewId: null,
        sourceVolumeId: 'root',
        onComplete: () => {},
        onCancelled: () => {},
        onError: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `TransferConflictDialog.svelte`.
 *
 * The conflict-resolution UI extracted from `TransferProgressDialog`: a
 * source-vs-destination comparison grid plus the resolution button grid and a
 * bottom Rollback/Cancel row. It's props-driven (no Tauri coupling), so each
 * test renders one conflict shape and audits the initial render.
 */
describe('TransferConflictDialog a11y', () => {
  beforeEach(() => {
    stubs.formatFileSize = (n: number) => `${String(n)} B`
  })

  function fileConflict(overrides: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
    return {
      operationId: 'op-1',
      conflictId: 1,
      sourcePath: '/Users/test/report.pdf',
      destinationPath: '/Users/test/dest/report.pdf',
      sourceSize: 2048,
      destinationSize: 1024,
      sourceModified: 1_700_000_000,
      destinationModified: 1_699_000_000,
      destinationIsNewer: false,
      sizeDifference: -1024,
      ...overrides,
    }
  }

  function mountDialog(opts: {
    conflictEvent: WriteConflictEvent
    isCopy: boolean
    isMove: boolean
    rollbackUnavailable: boolean
  }): HTMLElement {
    const target = container()
    mount(TransferConflictDialog, {
      target,
      props: {
        conflictEvent: opts.conflictEvent,
        isCopy: opts.isCopy,
        isMove: opts.isMove,
        rollbackUnavailable: opts.rollbackUnavailable,
        isCancelling: false,
        isResolvingConflict: false,
        onResolve: () => {},
        onCancel: () => {},
      },
    })
    return target
  }

  it('file-over-file copy conflict has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict(),
      isCopy: true,
      isMove: false,
      rollbackUnavailable: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('same-volume move (rollback disabled) has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict(),
      isCopy: false,
      isMove: true,
      rollbackUnavailable: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('type-mismatch (folder over file) conflict has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict({
        sourceIsDirectory: true,
        destinationIsDirectory: false,
        sourceSize: null,
        sizeDifference: null,
      }),
      isCopy: true,
      isMove: false,
      rollbackUnavailable: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('unknown sizes (network/MTP destination) conflict has no a11y violations', async () => {
    const target = mountDialog({
      conflictEvent: fileConflict({
        sourceSize: null,
        destinationSize: null,
        sourceModified: null,
        destinationModified: null,
        sizeDifference: null,
      }),
      isCopy: false,
      isMove: true,
      rollbackUnavailable: false,
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `TransferErrorDialog.svelte`.
 *
 * `alertdialog` role with an error title, message, suggestion, and a
 * collapsible "Technical details" section. Tests cover multiple error
 * types (permission_denied, read_only_device, insufficient_space,
 * device_disconnected) and operation types.
 */
describe('TransferErrorDialog a11y', () => {
  it('permission_denied (copy, close-only) has no a11y violations', async () => {
    const target = container()
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'copy',
        error: { type: 'permission_denied', path: '/Users/test/protected.txt', message: 'EACCES' },
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('insufficient_space (move) with retry has no a11y violations', async () => {
    const target = container()
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'move',
        error: {
          type: 'insufficient_space',
          required: 1024 * 1024 * 500,
          available: 1024 * 1024 * 42,
          volumeName: 'External',
        },
        onClose: () => {},
        onRetry: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('read_only_device (delete) has no a11y violations', async () => {
    const target = container()
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'delete',
        error: { type: 'read_only_device', path: '/Volumes/ReadOnly/file.txt', deviceName: 'ReadOnly' },
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('device_disconnected (trash) has no a11y violations', async () => {
    const target = container()
    mount(TransferErrorDialog, {
      target,
      props: {
        operationType: 'trash',
        error: { type: 'device_disconnected', path: '/Volumes/External/file.txt' },
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
