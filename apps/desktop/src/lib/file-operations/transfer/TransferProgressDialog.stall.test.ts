/**
 * The stall notice, rendered.
 *
 * `transfer-stall.test.ts` pins WHEN the notice should appear (the pure
 * decision); this pins what the dialog does with it: that a stalled transfer
 * shows it, that it sits at the foot of the body directly above the actions
 * (it's the reason a person reaches for Cancel), and that a healthy transfer
 * never sees it. Plus an axe pass over the stalled state, which the tier-3
 * suite can't reach because it only renders the just-mounted dialog.
 *
 * The window's session registry is inited per test: it is what subscribes the
 * event fan-out (through the mocked helpers below), and the mounted dialog
 * binds to the session for its operation. The test then drives a synthesised
 * progress event carrying a `TransferActivity` through the captured callback.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot, TransferActivity, WriteProgressEvent } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import TransferProgressDialog from './TransferProgressDialog.svelte'

let progressCb: ((e: WriteProgressEvent) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  copyFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn((cb: (e: WriteProgressEvent) => void) => {
    progressCb = cb
    return Promise.resolve(() => {
      progressCb = null
    })
  }),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  cancelOperation: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  // The backend registers the operation before the start command returns, so a
  // session that seeds itself finds it. An empty list would tell the session the
  // transfer was already over.
  listOperations: vi.fn(() =>
    Promise.resolve<OperationSnapshot[]>([
      {
        operationId: 'op-1',
        operationType: 'copy',
        status: 'running',
        source: '/Users/test',
        destination: '/Users/test/dest',
        supportsRollback: true,
        error: null,
      },
    ]),
  ),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 500),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

async function flushPromises(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

function copyingEvent(activity: TransferActivity): WriteProgressEvent {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'holiday.mov',
    filesDone: 35,
    filesTotal: 119204,
    bytesDone: 113_000_000,
    bytesTotal: 333_000_000_000,
    dirsDone: 0,
    bytesPerSecond: null,
    filesPerSecond: null,
    etaSeconds: null,
    activity,
  }
}

beforeEach(async () => {
  // Reset before the fan-out subscribes: the registry is what listens now, and
  // it has to be up before a dialog binds to a session.
  progressCb = null
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

async function mountDialog(): Promise<{ component: ReturnType<typeof mount>; target: HTMLDivElement }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(TransferProgressDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/file.txt'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/test/dest',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      onComplete: vi.fn(),
      onCancelled: vi.fn(),
      onError: vi.fn(),
    },
  })
  await flushPromises()
  return { component, target }
}

/** The card the notice renders into, or `null` when nothing is showing. */
function stallCard(target: HTMLElement): HTMLElement | null {
  return target.querySelector('.stall-notice .section-card')
}

describe('TransferProgressDialog stall notice', () => {
  it('shows a warning-toned notice once the backend says the transfer stopped moving', async () => {
    const { component, target } = await mountDialog()
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(copyingEvent({ inFlight: 1, stillForSeconds: 23, waitingOn: 'destination' }))
    await tick()

    const card = stallCard(target)
    expect(card, 'the notice renders').toBeTruthy()
    // The house warning surface, not a hand-picked yellow: `SectionCard`'s tone
    // owns the fill and border, and it's the same one the conflict block uses.
    expect(card?.getAttribute('data-tone')).toBe('warning')
    void unmount(component)
  })

  it('puts the notice below the current-file line and above the actions', async () => {
    const { component, target } = await mountDialog()
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(copyingEvent({ inFlight: 1, stillForSeconds: 23, waitingOn: 'destination' }))
    await tick()

    const notice = target.querySelector('.stall-notice')
    const currentFile = target.querySelector('.current-file')
    const buttons = target.querySelector('.button-row')
    expect(notice && currentFile && buttons, 'all three blocks render').toBeTruthy()
    if (!notice || !currentFile || !buttons) throw new Error('missing blocks')
    // `DOCUMENT_POSITION_FOLLOWING` reads "the argument comes after this node".
    // The notice belongs at the foot of the body, not wedged into the readout.
    expect(currentFile.compareDocumentPosition(notice) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(notice.compareDocumentPosition(buttons) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    void unmount(component)
  })

  it('stays silent while the transfer is moving', async () => {
    const { component, target } = await mountDialog()
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(copyingEvent({ inFlight: 2, stillForSeconds: 0, waitingOn: 'moving' }))
    await tick()

    expect(stallCard(target), 'a healthy transfer is never accused of stalling').toBeNull()
    void unmount(component)
  })

  it('has no a11y violations while stalled', async () => {
    const { component, target } = await mountDialog()
    if (!progressCb) throw new Error('subscriber never registered')

    progressCb(copyingEvent({ inFlight: 1, stillForSeconds: 23, waitingOn: 'source' }))
    await tick()

    await expectNoA11yViolations(target)
    void unmount(component)
  })
})
