/**
 * Rollback-affordance matrix for `TransferProgressDialog.svelte`.
 *
 * Same-volume volume moves (one smb2 share / one MTP device, `volume.rename`
 * based) have NO backend rollback — the engine stops without reversing. So the
 * dialog DISABLES Rollback (with an explanatory tooltip) on that path, while a
 * plain Cancel stays reachable. Every other copy/move keeps a live Rollback.
 *
 * Two Rollback affordances exist:
 *   - the conflict-section footer (visible while a `write-conflict` is showing),
 *   - the main footer (visible during the normal progress phase).
 * Both must apply the disable+tooltip consistently. These tests drive both.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { OperationSnapshot, WriteConflictEvent } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'
import {
  destroyOperationSessions,
  initOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import TransferProgressDialog from './TransferProgressDialog.svelte'

const ROLLBACK_TOOLTIP = 'Rollback is not available for same-volume moves'
const ALREADY_LANDED_TOOLTIP =
  "Every file is already at the destination, so Cmdr can't undo the move now. Cancel still stops it from removing the rest of the originals."

let conflictCb: ((e: WriteConflictEvent) => void) | null = null

// The dialog's phase comes from `write-progress`, and it opens in `scanning`
// (the operation is registered before its preview finishes). Rollback is
// deliberately disabled while nothing has been written, so the harness drives a
// copying tick to reach the state these tests are about.
//
// `registry` is what the operation's own row says about itself: the dialog reads
// `supportsRollback` off it, so a test that wants the backend's verdict rather
// than the props-only rule sets it here.
const { progressCbs, registry } = vi.hoisted(() => ({
  progressCbs: [] as ((event: Record<string, unknown>) => void)[],
  registry: { supportsRollback: true },
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveBetweenVolumes: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  moveFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  deleteFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  trashFiles: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  onWriteProgress: vi.fn((cb: (event: Record<string, unknown>) => void) => {
    progressCbs.push(cb)
    return Promise.resolve(() => {})
  }),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
    conflictCb = cb
    return Promise.resolve(() => {
      conflictCb = null
    })
  }),
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
        supportsRollback: registry.supportsRollback,
        reverses: null,
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
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'smb-share-1', name: 'NAS', path: '/Volumes/NAS', category: 'network', isEjectable: false },
  ],
}))

interface MountOptions {
  operationType: 'copy' | 'move'
  sourceVolumeId: string
  destVolumeId: string
  /** The phase the harness ticks after mounting. `copying` is the middle of the
   *  work; `deleting` is a move between filesystems on its last stage, removing
   *  the originals after every file has landed. */
  phase?: 'copying' | 'deleting'
}

async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

/** Mounted dialogs, torn down individually in `beforeEach` so the tooltip
 *  module's shared `<body>` container (created lazily on first hover) survives
 *  between tests instead of being orphaned by a blanket `innerHTML = ''`.
 *  Unmounted rather than merely detached: a dialog is a view now, and one left
 *  mounted would keep holding a session for `op-1` into the next test. */
const mounted: { target: HTMLElement; instance: ReturnType<typeof mount> }[] = []

async function mountDialog(opts: MountOptions): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(TransferProgressDialog, {
    target,
    props: {
      operationType: opts.operationType,
      sourcePaths: ['/Users/test/things'],
      sourceFolderPath: '/Users/test',
      destinationPath: '/Users/test/dest',
      direction: 'right',
      sortColumn: 'name',
      sortOrder: 'ascending',
      previewId: null,
      sourceVolumeId: opts.sourceVolumeId,
      destVolumeId: opts.destVolumeId,
      conflictResolution: 'stop',
      onComplete: () => {},
      onCancelled: () => {},
      onError: () => {},
    },
  })
  mounted.push({ target, instance })
  await flushMicrotasks()
  await tickPhase(opts.phase ?? 'copying', opts.operationType)
  return target
}

/** Drives one `write-progress` tick, which is where the dialog's phase comes from. */
async function tickPhase(phase: 'copying' | 'deleting', operationType: 'copy' | 'move'): Promise<void> {
  for (const cb of [...progressCbs]) {
    cb({
      operationId: 'op-1',
      operationType,
      phase,
      currentFile: 'a.bin',
      filesDone: 1,
      filesTotal: 4,
      bytesDone: 25,
      bytesTotal: 100,
    })
  }
  await flushMicrotasks()
}

/** Fires a synthetic file conflict so the conflict-section footer renders. */
async function fireConflict(): Promise<void> {
  const cb = conflictCb
  if (cb === null) throw new Error('conflict subscriber never registered')
  cb({
    operationId: 'op-1',
    conflictId: 1,
    sourcePath: '/Users/test/things/report.pdf',
    destinationPath: '/Users/test/dest/report.pdf',
    sourceSize: 2048,
    destinationSize: 1024,
    sourceModified: 1_710_000_000,
    destinationModified: 1_700_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
    sourceIsDirectory: false,
    destinationIsDirectory: false,
  })
  await tick()
}

function buttonByText(target: HTMLElement, text: string): HTMLButtonElement | null {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('button'))
  return buttons.find((b) => b.textContent.trim() === text) ?? null
}

/** Rollback is offered for real: neither `disabled` nor blocked. Asserting only
 *  `disabled === false` would pass on a blocked button, which is switched off. */
function expectRollbackLive(target: HTMLElement): void {
  const rollback = buttonByText(target, 'Rollback')
  expect(rollback, 'Rollback button present').toBeTruthy()
  expect(rollback?.disabled).toBe(false)
  expect(rollback?.getAttribute('aria-disabled')).toBeNull()
}

/**
 * Hovers a tooltip host and returns the rendered tooltip text. The tooltip
 * action shows after a 400 ms delay, so we drive a fake timer past it. Reads
 * the shared `.cmdr-tooltip` element the action appends to <body>.
 */
function readTooltipOnHover(host: Element): string {
  vi.useFakeTimers()
  try {
    host.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }))
    vi.advanceTimersByTime(500)
    const tip = document.querySelector('.cmdr-tooltip')
    const text = tip?.textContent ?? ''
    return text.trim()
  } finally {
    host.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }))
    vi.useRealTimers()
  }
}

/**
 * Focuses a tooltip host and returns the rendered tooltip text, which is the
 * KEYBOARD path: a blocked button explains itself only if the explanation arrives
 * without a pointer. Shows immediately on focus (no hover delay).
 */
function readTooltipOnFocus(host: HTMLElement): string {
  vi.useFakeTimers()
  try {
    host.focus()
    expect(document.activeElement, 'the button never took focus, so nothing could read its tooltip').toBe(host)
    host.dispatchEvent(new FocusEvent('focus'))
    vi.advanceTimersByTime(500)
    return document.querySelector('.cmdr-tooltip')?.textContent.trim() ?? ''
  } finally {
    host.dispatchEvent(new FocusEvent('blur'))
    vi.useRealTimers()
  }
}

beforeEach(async () => {
  // Remove only mounted dialog targets, not the tooltip module's shared
  // <body> container — wiping it orphans the module's cached reference and the
  // next hover appends the tooltip to a detached node (queryable as null).
  while (mounted.length > 0) {
    const view = mounted.pop()
    if (!view) continue
    void unmount(view.instance)
    view.target.remove()
  }
  // Reset the captured callbacks BEFORE the fan-out subscribes: it is the
  // registry that listens now, and it has to be up before a dialog binds.
  conflictCb = null
  progressCbs.length = 0
  registry.supportsRollback = true
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

/* ------------------------------------------------------------------------- */
/* Main footer (progress phase)                                              */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — main footer', () => {
  it('disables Rollback for a same-volume volume move and shows the tooltip text', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback, 'Rollback button present').toBeTruthy()
    // Blocked, not `disabled`: the button carries the tooltip itself and stays in
    // the tab order, so the reason reaches a keyboard user too.
    expect(rollback?.getAttribute('aria-disabled')).toBe('true')
    expect(rollback?.disabled, 'a disabled button could not be focused to read the why').toBe(false)
    if (rollback) {
      expect(readTooltipOnHover(rollback)).toBe(ROLLBACK_TOOLTIP)
      expect(readTooltipOnFocus(rollback)).toBe(ROLLBACK_TOOLTIP)
    }
  })

  it('asks nothing when a blocked Rollback is pressed', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    buttonByText(target, 'Rollback')?.click()
    await tick()
    expect(
      target.querySelector('#rollback-confirmation-body'),
      'the confirmation would promise a reversal the backend never makes',
    ).toBeNull()
  })

  it('keeps Cancel reachable for a same-volume volume move', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    const cancel = buttonByText(target, 'Cancel')
    expect(cancel, 'Cancel button present').toBeTruthy()
    expect(cancel?.disabled).toBe(false)
  })

  it('keeps Rollback LIVE for a cross-volume move the registry says it can reverse', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'smb-share-1', destVolumeId: 'root' })
    expectRollbackLive(target)
  })

  it('blocks Rollback on a cross-volume move, whose own row says it cannot be reversed', async () => {
    // The props alone can't tell: source and destination are two different
    // volumes, which is the shape of a perfectly reversible transfer. The
    // operation's own `supportsRollback` is the authority, and the queue window
    // has always read it — pre-fix this dialog offered a button that only
    // cancelled.
    registry.supportsRollback = false
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'smb-share-1', destVolumeId: 'root' })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.getAttribute('aria-disabled')).toBe('true')
    if (rollback) expect(readTooltipOnFocus(rollback)).toBe(ROLLBACK_TOOLTIP)
  })

  it('keeps Rollback LIVE for a local→local same-FS move (default volume has real rollback)', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    expectRollbackLive(target)
  })

  it('keeps Rollback LIVE for a same-volume COPY (only moves are affected)', async () => {
    const target = await mountDialog({
      operationType: 'copy',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    expectRollbackLive(target)
  })

  it('has no a11y violations with the blocked Rollback', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await expectNoA11yViolations(target)
  })
})

/* ------------------------------------------------------------------------- */
/* What a live Rollback's tooltip promises                                   */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — the live tooltip', () => {
  it('tells a MOVE the files travel back, and never words it as a delete', async () => {
    // Pre-fix both operations showed the copy's "delete every file written so
    // far" over a reversal that deletes nothing.
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback).toBeTruthy()
    if (!rollback) return
    const tip = readTooltipOnHover(rollback)
    expect(tip).toContain('move back')
    expect(tip).not.toContain('delete')
  })

  it('keeps the delete wording for a COPY, which really does remove what it wrote', async () => {
    const target = await mountDialog({ operationType: 'copy', sourceVolumeId: 'root', destVolumeId: 'root' })
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback).toBeTruthy()
    if (rollback) expect(readTooltipOnHover(rollback)).toContain('delete')
  })
})

/* ------------------------------------------------------------------------- */
/* A move on its last stage: the originals are going                         */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — while a move removes the originals', () => {
  /** A local move between two filesystems, on its source-deletion phase: every
   *  file has landed at the destination, the engine has committed, and nothing
   *  can carry them home any more. */
  const sweeping = () =>
    mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root', phase: 'deleting' })

  it('blocks Rollback there, and says why plus what still works', async () => {
    const target = await sweeping()
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback, 'Rollback button present').toBeTruthy()
    if (!rollback) return
    expect(rollback.getAttribute('aria-disabled')).toBe('true')
    expect(rollback.disabled, 'a disabled button drops out of the tab order, taking the why with it').toBe(false)
    expect(readTooltipOnFocus(rollback)).toBe(ALREADY_LANDED_TOOLTIP)
  })

  it('leaves Cancel enabled, which is the button that still does something', async () => {
    const target = await sweeping()
    const cancel = buttonByText(target, 'Cancel')
    expect(cancel, 'Cancel button present').toBeTruthy()
    expect(cancel?.disabled).toBe(false)
  })

  it('asks nothing when the blocked Rollback is pressed', async () => {
    const target = await sweeping()
    buttonByText(target, 'Rollback')?.click()
    await tick()
    expect(target.querySelector('#rollback-confirmation-body')).toBeNull()
  })

  it('keeps Rollback live through the phases before it', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    expectRollbackLive(target)
  })

  it('withdraws the question if the move lands while it is up', async () => {
    // The confirmation promises "this moves back everything the operation has
    // moved so far". The moment the sweep starts that promise is false, so the
    // question goes with it rather than waiting for an answer nobody can honor.
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    buttonByText(target, 'Rollback')?.click()
    await tick()
    expect(target.querySelector('#rollback-confirmation-body'), 'the question is up').not.toBeNull()

    await tickPhase('deleting', 'move')
    expect(target.querySelector('#rollback-confirmation-body')).toBeNull()
  })

  it('has no a11y violations with the blocked Rollback', async () => {
    const target = await sweeping()
    await expectNoA11yViolations(target)
  })
})

/* ------------------------------------------------------------------------- */
/* Conflict-section footer                                                   */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — conflict-section footer', () => {
  it('disables Rollback and shows a reachable Cancel for a same-volume volume move', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await fireConflict()
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback, 'Rollback present in conflict footer').toBeTruthy()
    expect(rollback?.getAttribute('aria-disabled')).toBe('true')
    if (rollback) {
      expect(readTooltipOnHover(rollback)).toBe(ROLLBACK_TOOLTIP)
      expect(readTooltipOnFocus(rollback)).toBe(ROLLBACK_TOOLTIP)
    }
    // Plain Cancel must be available alongside it so the user can still back out.
    const cancel = buttonByText(target, 'Cancel')
    expect(cancel, 'Cancel present in conflict footer').toBeTruthy()
    expect(cancel?.disabled).toBe(false)
  })

  it('blocks Rollback on a cross-volume move, the same verdict the main footer reads', async () => {
    // The two footers ask the operation, not the volume ids, so a clash answered
    // mid-transfer can't be the one moment Rollback looks available.
    registry.supportsRollback = false
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'smb-share-1', destVolumeId: 'root' })
    await fireConflict()
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback?.getAttribute('aria-disabled')).toBe('true')
    const cancel = buttonByText(target, 'Cancel')
    expect(cancel, 'a plain Cancel takes its place as the way out').toBeTruthy()
    expect(cancel?.disabled).toBe(false)
  })

  it('keeps a live Rollback (no Cancel) for a cross-volume move conflict the registry can reverse', async () => {
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'smb-share-1', destVolumeId: 'root' })
    await fireConflict()
    expectRollbackLive(target)
    expect(buttonByText(target, 'Cancel')).toBeNull()
  })

  it('keeps a live Rollback for a same-volume COPY conflict', async () => {
    const target = await mountDialog({
      operationType: 'copy',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await fireConflict()
    expectRollbackLive(target)
  })

  it('has no a11y violations with the disabled Rollback in the conflict footer', async () => {
    const target = await mountDialog({
      operationType: 'move',
      sourceVolumeId: 'smb-share-1',
      destVolumeId: 'smb-share-1',
    })
    await fireConflict()
    await expectNoA11yViolations(target)
  })
})

/* ------------------------------------------------------------------------- */
/* The question the Rollback button raises                                   */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog Rollback — what the confirmation says', () => {
  /** Presses the enabled Rollback and returns the stacked confirmation's body
   *  text plus the classes on its confirming button. */
  async function askRollback(target: HTMLElement): Promise<{ body: string; confirmClass: string }> {
    const rollback = buttonByText(target, 'Rollback')
    expect(rollback, 'Rollback button present').toBeTruthy()
    rollback?.click()
    await tick()
    const body = target.querySelector('#rollback-confirmation-body')?.textContent.trim() ?? ''
    const confirm = buttonByText(target, 'Roll back')
    expect(confirm, 'the confirming button is up').toBeTruthy()
    return { body, confirmClass: confirm?.className ?? '' }
  }

  it('tells a MOVE the files travel back, on a button that does not read as destructive', async () => {
    // Pre-fix this showed the copy's "this deletes every file the operation has
    // written so far" in red, over a reversal that deletes nothing.
    const target = await mountDialog({ operationType: 'move', sourceVolumeId: 'root', destVolumeId: 'root' })
    const { body, confirmClass } = await askRollback(target)
    expect(body).toContain('moves back')
    expect(body).not.toContain('deletes')
    expect(confirmClass).toContain('btn-primary')
    expect(confirmClass).not.toContain('btn-danger')
  })

  it('keeps the delete wording and the red button for a COPY, which really does delete what it wrote', async () => {
    const target = await mountDialog({ operationType: 'copy', sourceVolumeId: 'root', destVolumeId: 'root' })
    const { body, confirmClass } = await askRollback(target)
    expect(body).toContain('deletes')
    expect(confirmClass).toContain('btn-danger')
  })

  it('offers the same way out of both, so the safe answer reads alike whichever operation is running', async () => {
    for (const operationType of ['copy', 'move'] as const) {
      const target = await mountDialog({ operationType, sourceVolumeId: 'root', destVolumeId: 'root' })
      await askRollback(target)
      expect(buttonByText(target, 'Keep them'), `Keep them present for ${operationType}`).toBeTruthy()
    }
  })
})
