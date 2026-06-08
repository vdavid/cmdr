/**
 * Conflict-dialog rendering matrix for `TransferProgressDialog.svelte`.
 *
 * The dialog reuses one shape (filename, Existing/New rows, 4×2 button grid,
 * Rollback row) across every clash type. The four variants differ in:
 *   - Row labels (the type tag inside the "Existing:" / "New:" prefix)
 *   - A red warning block above the filename (file → folder only)
 *   - The "Overwrite" / "Overwrite all" button copy (file → folder only)
 *   - Whether the destination size is known (renders normally or "(unknown)")
 *   - Whether "Overwrite all smaller" is enabled (depends on destination size)
 *
 * We drive the dialog by capturing the `onWriteConflict` callback and firing
 * synthetic events that walk a 4-variant × known/unknown axis (with a single
 * a11y check per variant). describe.each over the axes keeps the matrix flat
 * and the runtime well under the per-test budget — each case mounts the
 * dialog component directly and asserts on the DOM.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { WriteConflictEvent } from '$lib/tauri-commands'
import { expectNoA11yViolations } from '$lib/test-a11y'
import TransferProgressDialog from './TransferProgressDialog.svelte'

let conflictCb: ((e: WriteConflictEvent) => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
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
  onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
    conflictCb = cb
    return Promise.resolve(() => {
      conflictCb = null
    })
  }),
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  formatDuration: vi.fn((s: number) => `${String(s)}s`),
  formatFilesPerSecond: vi.fn((r: number) => `${String(r)} files/s`),
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

async function flushMicrotasks(): Promise<void> {
  // The dialog's onMount runs ~6 `await onWriteX(...)` subscribers before it
  // reaches `await dispatchOperation()` and the conflict callback gets wired
  // up. We need enough microtask turns to walk the entire chain. Each round
  // here yields exactly one macrotask + one microtask flush + a Svelte tick;
  // 10 rounds is heavy overkill but still under 10 ms in jsdom.
  for (let i = 0; i < 10; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

async function mountDialogWithConflict(event: WriteConflictEvent): Promise<HTMLDivElement> {
  conflictCb = null
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferProgressDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/things'],
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
  await flushMicrotasks()
  // Cast into a const so TS narrowing doesn't get widened back to nullable
  // across any future await further down (another await could in theory
  // reassign `conflictCb`, even though we control the mock here).
  const cb = conflictCb as ((e: WriteConflictEvent) => void) | null
  if (cb === null) throw new Error('conflict subscriber never registered')
  cb(event)
  await tick()
  return target
}

function buttonByText(target: HTMLElement, text: string): HTMLButtonElement | null {
  const buttons = Array.from(target.querySelectorAll<HTMLButtonElement>('button'))
  return buttons.find((b) => b.textContent.trim() === text) ?? null
}

function makeEvent(overrides: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
  return {
    operationId: 'op-1',
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
    ...overrides,
  }
}

beforeEach(() => {
  conflictCb = null
  document.body.innerHTML = ''
})

/* ------------------------------------------------------------------------- */
/* Clash-type axis × destination-size-known axis                             */
/* ------------------------------------------------------------------------- */

interface VariantCase {
  name: string
  sourceIsDirectory: boolean
  destinationIsDirectory: boolean
  existingLabel: string
  newLabel: string
  hasWarning: boolean
  overwriteLabel: string
  overwriteAllLabel: string
}

const variants: VariantCase[] = [
  {
    name: 'file → file',
    sourceIsDirectory: false,
    destinationIsDirectory: false,
    existingLabel: 'Existing:',
    newLabel: 'New:',
    hasWarning: false,
    overwriteLabel: 'Overwrite',
    overwriteAllLabel: 'Overwrite all',
  },
  // NOTE: there is no `folder → folder` variant. Dir-vs-dir is never a conflict —
  // the volume merge engine always merges same-named folders silently and never
  // emits a `write-conflict` for the folder itself (see the BE merge engine and
  // `merge_tests`). The dedicated test below pins that a dir-dir event, if one
  // ever did arrive, would NOT render the cross-type red-warning path.
  {
    name: 'folder → file',
    sourceIsDirectory: true,
    destinationIsDirectory: false,
    existingLabel: 'Existing (file):',
    newLabel: 'New (folder):',
    hasWarning: false,
    overwriteLabel: 'Overwrite',
    overwriteAllLabel: 'Overwrite all',
  },
  {
    name: 'file → folder',
    sourceIsDirectory: false,
    destinationIsDirectory: true,
    existingLabel: 'Existing (folder):',
    newLabel: 'New (file):',
    hasWarning: true,
    overwriteLabel: 'Overwrite folder with file',
    overwriteAllLabel: 'Overwrite folders with files',
  },
]

describe.each(variants)('TransferProgressDialog conflict — $name', (variant) => {
  it('shows the baseline title and filename', async () => {
    const target = await mountDialogWithConflict(
      makeEvent({
        sourceIsDirectory: variant.sourceIsDirectory,
        destinationIsDirectory: variant.destinationIsDirectory,
      }),
    )
    expect(target.textContent).toContain('File already exists')
    expect(target.textContent).toContain('report.pdf')
  })

  it('renders Existing/New row labels with the right type tags', async () => {
    const target = await mountDialogWithConflict(
      makeEvent({
        sourceIsDirectory: variant.sourceIsDirectory,
        destinationIsDirectory: variant.destinationIsDirectory,
      }),
    )
    const labels = Array.from(target.querySelectorAll('.conflict-file-label')).map((l) => l.textContent.trim())
    expect(labels).toEqual([variant.existingLabel, variant.newLabel])
  })

  it(`${variant.hasWarning ? 'shows' : 'omits'} the red warning block`, async () => {
    const target = await mountDialogWithConflict(
      makeEvent({
        sourceIsDirectory: variant.sourceIsDirectory,
        destinationIsDirectory: variant.destinationIsDirectory,
      }),
    )
    const warning = target.querySelector('.conflict-warning')
    if (variant.hasWarning) {
      expect(warning, 'red warning block present').not.toBeNull()
      // Both bold spans render as real <strong> elements with the right text.
      const strongs = Array.from(warning?.querySelectorAll('strong') ?? []).map((s) => s.textContent.trim())
      expect(strongs).toEqual(['folder', 'file'])
      // Role + content sanity-check matches the spec verbiage.
      expect(warning?.getAttribute('role')).toBe('alert')
      expect(warning?.textContent).toContain('overwrite it with a')
      expect(warning?.textContent).toContain('What to do?')
    } else {
      expect(warning, 'no red warning block').toBeNull()
    }
  })

  it('uses the correct "Overwrite" button labels', async () => {
    const target = await mountDialogWithConflict(
      makeEvent({
        sourceIsDirectory: variant.sourceIsDirectory,
        destinationIsDirectory: variant.destinationIsDirectory,
      }),
    )
    expect(buttonByText(target, variant.overwriteLabel), variant.overwriteLabel).toBeTruthy()
    expect(buttonByText(target, variant.overwriteAllLabel), variant.overwriteAllLabel).toBeTruthy()
    // Both stay on the secondary variant — danger styling moved out per spec.
    expect(buttonByText(target, variant.overwriteLabel)?.classList.contains('btn-secondary')).toBe(true)
  })

  it('keeps Skip / Skip all / Rename / Rename all unchanged and enabled', async () => {
    const target = await mountDialogWithConflict(
      makeEvent({
        sourceIsDirectory: variant.sourceIsDirectory,
        destinationIsDirectory: variant.destinationIsDirectory,
      }),
    )
    for (const label of ['Skip', 'Skip all', 'Rename', 'Rename all']) {
      const btn = buttonByText(target, label)
      expect(btn, label).toBeTruthy()
      expect(btn?.disabled).toBe(false)
    }
  })

  it('has no a11y violations', async () => {
    const target = await mountDialogWithConflict(
      makeEvent({
        sourceIsDirectory: variant.sourceIsDirectory,
        destinationIsDirectory: variant.destinationIsDirectory,
      }),
    )
    await expectNoA11yViolations(target)
  })
})

/* ------------------------------------------------------------------------- */
/* Dir-vs-dir is never a conflict (folders always merge silently).           */
/* The BE no longer emits a folder→folder `write-conflict`; this pins that    */
/* the FE per-file dialog has no folder-merge prompt and would not show the   */
/* cross-type red warning even if a stray dir-dir event arrived.             */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog conflict — dir-vs-dir is not a conflict', () => {
  it('never renders the red cross-type warning for a folder→folder event', async () => {
    const target = await mountDialogWithConflict(makeEvent({ sourceIsDirectory: true, destinationIsDirectory: true }))
    // The cross-type warning block is the file↔folder destructive cue. A
    // dir-vs-dir clash is a merge, so it must never carry that warning.
    expect(target.querySelector('.conflict-warning'), 'dir-vs-dir must show no red warning').toBeNull()
    // And it never relabels Overwrite to the destructive cross-type wording.
    expect(buttonByText(target, 'Overwrite folder with file')).toBeNull()
    expect(buttonByText(target, 'Overwrite folders with files')).toBeNull()
  })
})

/* ------------------------------------------------------------------------- */
/* destinationSize known vs null — only meaningful for file → folder, where  */
/* the BE can legitimately fail to look up the destination folder size.      */
/* ------------------------------------------------------------------------- */

describe('TransferProgressDialog conflict — file → folder, destinationSize known', () => {
  const event = makeEvent({
    sourceIsDirectory: false,
    destinationIsDirectory: true,
    destinationSize: 4096,
    sizeDifference: 2048,
  })

  it('renders the destination size in the Existing slot (not "(unknown)")', async () => {
    const target = await mountDialogWithConflict(event)
    const existingSize = target.querySelector('.conflict-file .conflict-file-size')
    expect(existingSize?.textContent.trim()).toBe('4096 B')
    expect(existingSize?.classList.contains('unknown')).toBe(false)
  })

  it('keeps "Overwrite all smaller" enabled', async () => {
    const target = await mountDialogWithConflict(event)
    expect(buttonByText(target, 'Overwrite all smaller')?.disabled).toBe(false)
  })

  it('keeps "Overwrite all older" enabled', async () => {
    const target = await mountDialogWithConflict(event)
    expect(buttonByText(target, 'Overwrite all older')?.disabled).toBe(false)
  })
})

describe('TransferProgressDialog conflict — file → folder, destinationSize null', () => {
  const event = makeEvent({
    sourceIsDirectory: false,
    destinationIsDirectory: true,
    destinationSize: null,
    sizeDifference: null,
  })

  it('renders "(unknown)" in the Existing slot using the muted color class', async () => {
    const target = await mountDialogWithConflict(event)
    const existingSize = target.querySelector('.conflict-file .conflict-file-size')
    expect(existingSize?.textContent.trim()).toBe('(unknown)')
    expect(existingSize?.classList.contains('unknown')).toBe(true)
  })

  it('disables "Overwrite all smaller" and wraps it in a tooltip host', async () => {
    const target = await mountDialogWithConflict(event)
    const smaller = buttonByText(target, 'Overwrite all smaller')
    expect(smaller?.disabled).toBe(true)
    // The disabled button is wrapped in a `.conflict-button-wrap` so the
    // tooltip action has a host to attach hover handlers to (disabled buttons
    // don't fire pointer events themselves). The tooltip text isn't reflected
    // in the DOM until hover, so we only assert the wrap is there.
    const wrap = smaller?.closest('.conflict-button-wrap')
    expect(wrap, 'tooltip host wrap present').not.toBeNull()
  })

  it('keeps "Overwrite all older" enabled (mtime is always known)', async () => {
    const target = await mountDialogWithConflict(event)
    expect(buttonByText(target, 'Overwrite all older')?.disabled).toBe(false)
  })

  it('still shows the red warning block (this is the file → folder variant)', async () => {
    const target = await mountDialogWithConflict(event)
    expect(target.querySelector('.conflict-warning')).not.toBeNull()
  })

  it('has no a11y violations with (unknown) destination size', async () => {
    const target = await mountDialogWithConflict(event)
    await expectNoA11yViolations(target)
  })
})

describe('TransferProgressDialog conflict — folder → file, sourceSize null', () => {
  // A folder source on a path with no pre-flight scan (the same-volume move
  // fast path) carries `sourceSize: null`. The New slot must render `(unknown)`
  // exactly the way the Existing slot does for a null destination size.
  const event = makeEvent({
    sourceIsDirectory: true,
    destinationIsDirectory: false,
    sourceSize: null,
    destinationSize: 1024,
    sizeDifference: null,
  })

  it('renders "(unknown)" in the New slot using the muted color class', async () => {
    const target = await mountDialogWithConflict(event)
    const sizes = target.querySelectorAll('.conflict-file .conflict-file-size')
    // [0] = Existing (file), [1] = New (folder).
    expect(sizes.length).toBe(2)
    const newSize = sizes[1]
    expect(newSize.textContent.trim()).toBe('(unknown)')
    expect(newSize.classList.contains('unknown')).toBe(true)
  })

  it('still renders the known destination size in the Existing slot', async () => {
    const target = await mountDialogWithConflict(event)
    const existingSize = target.querySelector('.conflict-file .conflict-file-size')
    expect(existingSize?.textContent.trim()).toBe('1024 B')
    expect(existingSize?.classList.contains('unknown')).toBe(false)
  })

  it('has no a11y violations with (unknown) source size', async () => {
    const target = await mountDialogWithConflict(event)
    await expectNoA11yViolations(target)
  })
})
