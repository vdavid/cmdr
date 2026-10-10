/** Shared IPC doubles and mount helpers for TransferDialog component tests. */
import { vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import * as commands from '$lib/tauri-commands'
import type { VolumeConflictInfo } from '$lib/tauri-commands'
import type { TransferConfirmPayload, TransferConfirmer } from '$lib/file-explorer/pane/dialog-props'

export const startScanPreviewMock = vi.mocked(commands.startScanPreview)
export const cancelScanPreviewMock = vi.mocked(commands.cancelScanPreview)
export const estimateOperationCostMock = vi.mocked(commands.estimateOperationCost)

// Captured scan-preview-complete callback, so a test can decide WHEN the
// (slow) byte scan finishes relative to the conflict check.
export let scanCompleteCb: ((e: ScanCompleteEvent) => void) | null = null

interface ScanCompleteEvent {
  previewId: string
  filesTotal: number
  dirsTotal: number
  bytesTotal: number
  dedupBytesTotal: number
}

// `scanVolumeForConflicts`'s real signature (`mtp.ts`) is positional all the way down to
// the raw IPC binding, so the exposed mock (below, in the `$lib/tauri-commands` factory)
// stays positional too; this inner mock is what tests actually assert against, as a named
// payload so a future edit can't silently swap `volumeId` and `destPath`.
export const scanVolumeForConflictsMock = vi.fn<
  (payload: {
    volumeId: string
    sourceItems: unknown[]
    destPath: string
    sourceVolumeId?: string
    sourcePaths?: string[]
  }) => Promise<VolumeConflictInfo[]>
>(() => Promise.resolve([]))

// Destination-existence probe (`destinationExists`, which counts a name the
// volume holds in another Unicode spelling) behind the "this folder will be
// created" warning.
// Defaults to "exists" so most tests see no warning; a test overrides it. Same
// positional-real-signature reasoning as `scanVolumeForConflictsMock` above.
export const destinationExistsMock = vi.fn<
  (payload: { path: string; volumeId?: string }) => Promise<{ data: boolean; timedOut: boolean }>
>(() => Promise.resolve({ data: true, timedOut: false }))

// Whether the destination folder takes writes, behind the red "nothing can go
// here" notice. Defaults to "can't tell" so most tests see no notice.
type WriteAccessAnswer =
  | { kind: 'writable' }
  | { kind: 'unwritable'; reason: 'readOnlyFilesystem' | 'noPermission' | 'unexplained' }
  | { kind: 'unknown' }
export const destinationWriteAccessMock = vi.fn<
  (payload: { volumeId: string; path: string }) => Promise<WriteAccessAnswer>
>(() => Promise.resolve({ kind: 'unknown' }))

// Both readings of a destination that repeats the place's own root folder (#164).
// Defaults to "reads one way" so most tests see no warning.
interface RootEchoAnswer {
  rootFolder: string
  resolved: string
  stripped: string
}
export const destinationRootEchoMock = vi.fn<
  (payload: { volumeId: string; path: string }) => Promise<RootEchoAnswer | null>
>(() => Promise.resolve(null))

// Home dir resolution for the long-form display of a bare `~` destination.
vi.mock('@tauri-apps/api/path', () => ({
  homeDir: () => Promise.resolve('/Users/test'),
}))

vi.mock('$lib/tauri-commands', () => ({
  estimateOperationCost: vi.fn(() => Promise.resolve([])),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() => Promise.resolve({ data: { totalBytes: 1_073_741_824, availableBytes: 524_288_000 } })),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  // Returns null so the dialog keeps waiting on the (captured) complete event
  // instead of hydrating from cached totals — lets a test hold the byte scan
  // open while the conflict check resolves.
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn((cb: (e: ScanCompleteEvent) => void) => {
    scanCompleteCb = cb
    return Promise.resolve(() => {
      scanCompleteCb = null
    })
  }),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: (
    volumeId: string,
    sourceItems: unknown[],
    destPath: string,
    sourceVolumeId?: string,
    sourcePaths?: string[],
  ) => scanVolumeForConflictsMock({ volumeId, sourceItems, destPath, sourceVolumeId, sourcePaths }),
  destinationExists: (path: string, volumeId?: string) => destinationExistsMock({ path, volumeId }),
  destinationWriteAccess: (volumeId: string, path: string) => destinationWriteAccessMock({ volumeId, path }),
  destinationRootEcho: (volumeId: string, path: string) => destinationRootEchoMock({ volumeId, path }),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => (key === 'behavior.archiveCompressionLevel' ? 6 : 500)),
  // Compress mode renders `CompressLevelControl` → `SettingSlider`, which reads
  // its metadata and default through the barrel and writes via `setSetting`.
  setSetting: vi.fn(),
  getDefaultValue: vi.fn(() => 6),
  onSpecificSettingChange: vi.fn(() => () => {}),
  getSettingDefinition: vi.fn(() => ({
    label: 'Compression level',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  })),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'ext', name: 'External', path: '/Volumes/External', category: 'attached_volume', isEjectable: true },
    {
      id: 'mtp-336592896:65538',
      name: 'Virtual Pixel 9 - SD Card',
      path: '/mtp-20-5/65538',
      category: 'mobile_device',
      isEjectable: true,
    },
    {
      id: 'smb://nas.local/public',
      name: 'NAS share',
      path: 'smb://nas.local/public',
      category: 'network',
      isEjectable: false,
    },
    {
      id: 's3-photos',
      name: 'photos',
      path: 's3://photos',
      category: 'attached_volume',
      fsType: 's3',
      isEjectable: false,
      capabilities: {
        backendCanWrite: true,
        canExport: true,
        canShareLinks: true,
        canBeIndexed: false,
        renamesCanCopy: true,
        hasOsMountFallback: false,
      },
    },
  ],
}))

export function makeConflict(overrides: Partial<VolumeConflictInfo>): VolumeConflictInfo {
  return {
    sourcePath: 'item',
    destPath: 'item',
    sourceSize: 0,
    destSize: 0,
    sourceModified: null,
    destModified: null,
    sourceIsDirectory: false,
    destIsDirectory: false,
    ...overrides,
  }
}

export async function flushMicrotasks(rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

interface MountOpts {
  autoConfirm?: boolean
  autoConfirmOnConflict?: string
  onConfirm?: ConfirmFn
  onCancel?: () => void
  operationType?: 'copy' | 'move' | 'compress'
  sourceVolumeId?: string
  /** The destination volume the dialog starts on (= `selectedVolumeId`). */
  currentVolumeId?: string
  sourceFolderPath?: string
  destinationPath?: string
  /** Rename mode (F2 on a big S3 folder): one source, renamed in place. */
  newName?: string
  sourcePaths?: string[]
  /** Takes the dialog's own confirm, as the MCP `dialog confirm` does. */
  registerConfirmer?: (confirm: TransferConfirmer) => () => void
}

export type ConfirmFn = (payload: TransferConfirmPayload) => void

const cleanupDialogs: Array<() => Promise<void>> = []

export function mountDialog(opts: MountOpts = {}): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const dialog = mount(TransferDialog, {
    target,
    props: {
      operationType: opts.operationType ?? 'copy',
      sourcePaths: opts.sourcePaths ?? ['/Users/test/photos', '/Users/test/notes.txt'],
      destinationPath: opts.destinationPath ?? '/Users/test/dest',
      currentVolumeId: opts.currentVolumeId ?? 'root',
      fileCount: 1,
      folderCount: 1,
      sourceFolderPath: opts.sourceFolderPath ?? '/Users/test',
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: opts.sourceVolumeId ?? 'root',
      destVolumeId: opts.currentVolumeId ?? 'root',
      autoConfirm: opts.autoConfirm ?? false,
      autoConfirmOnConflict: opts.autoConfirmOnConflict,
      newName: opts.newName,
      onConfirm: opts.onConfirm ?? (() => {}),
      registerConfirmer: opts.registerConfirmer,
      onCancel: opts.onCancel ?? (() => {}),
    },
  })
  cleanupDialogs.push(() => unmount(dialog))
  return target
}

export function radioGroup(target: HTMLElement): HTMLElement | null {
  return target.querySelector('.conflict-policy')
}

/** Reads the `data-scan-state` marker off the tallies element. */
export function scanState(target: HTMLElement): string | null {
  return target.querySelector('.scan-stats')?.getAttribute('data-scan-state') ?? null
}

beforeEach(() => {
  scanCompleteCb = null
  scanVolumeForConflictsMock.mockReset()
  scanVolumeForConflictsMock.mockResolvedValue([])
  destinationExistsMock.mockReset()
  destinationExistsMock.mockResolvedValue({ data: true, timedOut: false })
  destinationWriteAccessMock.mockReset()
  destinationWriteAccessMock.mockResolvedValue({ kind: 'unknown' })
  destinationRootEchoMock.mockReset()
  destinationRootEchoMock.mockResolvedValue(null)
  startScanPreviewMock.mockClear()
  startScanPreviewMock.mockResolvedValue({ previewId: 'preview-1' })
  cancelScanPreviewMock.mockClear()
  document.body.innerHTML = ''
})

afterEach(async () => {
  await Promise.all(cleanupDialogs.splice(0).map((cleanup) => cleanup()))
  document.body.innerHTML = ''
})

/** A promise plus its resolver, so a test decides exactly when an async
 *  dependency settles (and can leave it pending indefinitely). */
export function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

export function confirmButton(target: HTMLElement): HTMLButtonElement {
  const btn = target.querySelector<HTMLButtonElement>('.btn-primary')
  if (!btn) throw new Error('confirm button not rendered')
  return btn
}

export function cancelButton(target: HTMLElement): HTMLButtonElement {
  const btn = Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(
    (b) => b.textContent.trim() === 'Cancel',
  )
  if (!btn) throw new Error('cancel button not rendered')
  return btn
}

/** The `×` in the dialog chrome. It calls `ModalDialog`'s `onclose` (= `handleCancel`)
 *  directly and is never disabled, so it's the honest way to drive the close path in a
 *  test — unlike the Cancel button, whose `disabled` would swallow the click. */
export function closeButton(target: HTMLElement): HTMLButtonElement {
  const btn = target.querySelector<HTMLButtonElement>('.modal-close-button')
  if (!btn) throw new Error('modal close button not rendered')
  return btn
}

/** Clicks the Copy/Move segmented toggle option by its label. */
export function clickToggle(target: HTMLElement, label: 'Copy' | 'Move'): void {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('.tg-root .tg-item'))
  const btn = buttons.find((b) => b.textContent.trim() === label)
  if (!btn) throw new Error(`toggle option "${label}" not found`)
  btn.click()
}

export function pathInput(target: HTMLElement): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>('input[aria-label="Destination path"]')
  if (!input) throw new Error('path input not found')
  return input
}
