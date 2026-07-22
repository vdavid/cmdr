/**
 * The harness's render decision: it shows the requested fixture, and shows
 * NOTHING (with a warning) when a request doesn't resolve to one. Half-filled
 * dialogs are the one thing a design-review instrument must never produce.
 *
 * The sweep at the bottom is the real guarantee: EVERY state of every `ready`
 * row actually mounts its dialog. A gallery that lists a state it can't open is
 * worse than one that doesn't list it, and clicking 40-odd buttons by hand after
 * each change isn't a plan.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { writable } from 'svelte/store'
import DialogGallery from './DialogGallery.svelte'
import { closeGalleryDialog, openGalleryDialog, type GalleryDiskFixture } from './gallery-state.svelte'
import { DIALOG_GALLERY_ENTRIES } from './gallery-registry'
import { notifyDialogOpened } from '$lib/tauri-commands'

// The gallery pulls in 21 shipping dialogs, so this mock covers every IPC any of
// them touches on mount (or would touch from a button). Resolved values only
// matter where a dialog renders them; the rest just have to not reject.
vi.mock('$lib/tauri-commands', () => ({
  DEFAULT_VOLUME_ID: 'root',
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  // The disk-backed dialogs: scan preview, space, conflict lookups, suggestions.
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'gallery-scan', cached: false })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  getVolumeSpace: vi.fn(() => Promise.resolve(null)),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('unavailable')),
  streamFolderSuggestions: vi.fn(() => ({ promise: Promise.resolve(), cancel: vi.fn() })),
  createDirectory: vi.fn(() => Promise.resolve()),
  createFile: vi.fn(() => Promise.resolve()),
  refreshListing: vi.fn(() => Promise.resolve()),
  isIpcError: vi.fn(() => false),
  resolveGoToPath: vi.fn(() => Promise.resolve({ status: 'ok', data: { kind: 'directory' } })),
  readClipboardText: vi.fn(() => Promise.resolve(null)),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  markCommercialReminderDismissed: vi.fn(() => Promise.resolve()),
  markExpirationModalShown: vi.fn(() => Promise.resolve()),
  sendCrashReport: vi.fn(() => Promise.resolve()),
  dismissCrashReport: vi.fn(() => Promise.resolve()),
  getPtpcameradWorkaroundCommand: vi.fn(() => Promise.resolve('sudo killall ptpcamerad')),
  connectToServer: vi.fn(() => Promise.resolve({ host: null, sharePath: null })),
  ensureNetworkDiscoveryStarted: vi.fn(() => Promise.resolve()),
  verifyLicense: vi.fn(() => Promise.resolve(null)),
  commitLicense: vi.fn(() => Promise.resolve(null)),
  validateLicenseWithServer: vi.fn(() => Promise.resolve(null)),
  getLicenseInfo: vi.fn(() => Promise.resolve(null)),
  resetLicense: vi.fn(() => Promise.resolve(null)),
  parseActivationError: vi.fn(() => ({ message: '' })),
  translateSelectionQuery: vi.fn(() => Promise.resolve(null)),
  addRecentSelection: vi.fn(() => Promise.resolve()),
  removeRecentSelection: vi.fn(() => Promise.resolve()),
  getRecentSelections: vi.fn(() => Promise.resolve([])),
  trackEvent: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
}))

const warn = vi.fn()
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    warn: (...args: unknown[]) => {
      warn(...args)
    },
    info: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}))

/** The harness's own "this state resolved to nothing" message. */
const NO_FIXTURE_WARNING = 'Dialog gallery has no fixture for {dialogId} / {stateId}'

// The store is module-level, so a harness left mounted from an earlier test would
// react to the next test's store writes (and re-warn). Every mount is torn down.
let mounted: Record<string, unknown> | undefined

function mountGallery(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(DialogGallery, { target })
  return target
}

beforeEach(() => {
  warn.mockClear()
  vi.mocked(notifyDialogOpened).mockClear()
})

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = undefined
  closeGalleryDialog()
})

describe('DialogGallery', () => {
  it('renders nothing while no preview is open', async () => {
    const target = mountGallery()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
    expect(warn).not.toHaveBeenCalled()
  })

  it('renders the requested alert fixture', async () => {
    openGalleryDialog('alert', 'short')
    const target = mountGallery()
    await tick()
    const dialog = target.querySelector('[role="alertdialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog?.textContent).toContain('Nothing to copy')
    expect(warn).not.toHaveBeenCalled()
  })

  it('swaps to another state without leaving the previous one mounted', async () => {
    openGalleryDialog('alert', 'short')
    const target = mountGallery()
    await tick()
    openGalleryDialog('alert', 'custom-button')
    await tick()
    expect(target.querySelectorAll('[role="alertdialog"]')).toHaveLength(1)
    expect(target.textContent).toContain('Indexing paused')
    expect(target.textContent).not.toContain('Nothing to copy')
  })

  it('renders nothing and warns when the state id has no fixture', async () => {
    openGalleryDialog('alert', 'no-such-state')
    const target = mountGallery()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('renders nothing and warns for a dialog the harness has no case for', async () => {
    openGalleryDialog('whats-new', 'default')
    const target = mountGallery()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('remounts the dialog when swapping between two states of the SAME dialog', async () => {
    // Re-rendering with new props would leave the delete dialog's scan state from
    // the previous trigger on screen next to the new state's file list: a real
    // number, but not of what you're looking at. A remount reports to the tracker
    // again, so two calls is the proof. Production keys these dialogs too.
    openGalleryDialog('delete-confirmation', 'trash-many', diskFixture)
    mountGallery()
    await tick()
    openGalleryDialog('delete-confirmation', 'permanent-single', diskFixture)
    await tick()

    const opens = vi.mocked(notifyDialogOpened).mock.calls.filter(([id]) => id === 'delete-confirmation')
    expect(opens).toHaveLength(2)
  })

  it('closes back to rendering nothing', async () => {
    openGalleryDialog('alert', 'short')
    const target = mountGallery()
    await tick()
    closeGalleryDialog()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
  })
})

/**
 * Stand-in for the fixture directory the dev-only Rust command creates and
 * `disk-fixture.ts` resolves from the focused pane. The real thing needs a
 * running app; what this pins is that every disk-backed state builds props the
 * dialog can mount from.
 */
const diskFixture: GalleryDiskFixture = {
  root: '/tmp/dialog-gallery-fixtures',
  destinationDir: '/tmp/dialog-gallery-fixtures/Backup destination',
  existingFolderName: 'Photos',
  existingFileName: 'Invoice 2026-07.pdf',
  nestedPath: '/tmp/dialog-gallery-fixtures/Projects/cmdr/src-tauri/src/file_system',
  paneSide: 'left',
  listingId: 'gallery-listing',
  volumeId: 'root',
  showHiddenFiles: false,
  sortColumn: 'name',
  sortOrder: 'ascending',
  entries: ['Photos', 'Projects', 'README.txt', 'Invoice 2026-07.pdf', 'Videos', 'Documents'].map((name, index) => ({
    name,
    path: `/tmp/dialog-gallery-fixtures/${name}`,
    isDirectory: index !== 2 && index !== 3,
    isSymlink: false,
    size: 4_096 * (index + 1),
    permissions: 0o644,
    owner: 'demo',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: false,
  })),
}

/** Every `[dialogId, stateId]` the Debug list offers a button for. */
const readyStates = DIALOG_GALLERY_ENTRIES.filter((entry) => entry.status === 'ready').flatMap((entry) =>
  entry.states.map((state) => [entry.dialogId, state.id, entry.usesFixtureDir === true] as const),
)

describe('every advertised gallery state opens its dialog', () => {
  it.each(readyStates)('%s / %s', async (dialogId, stateId, usesFixtureDir) => {
    openGalleryDialog(dialogId, stateId, usesFixtureDir ? diskFixture : undefined)
    const target = mountGallery()
    await tick()
    // Every soft dialog reports its own mount to the Rust tracker, so this
    // asserts the dialog the row CLAIMS actually came up, not merely that
    // something rendered. `QueryDialog` reports from its own overlay rather than
    // `ModalDialog`, so a `data-dialog-id` selector wouldn't cover all of them.
    expect(vi.mocked(notifyDialogOpened).mock.calls.map(([id]) => id)).toContain(dialogId)
    expect(target.childElementCount, 'nothing rendered').toBeGreaterThan(0)
    // Other modules log their own warnings here (settings reads before init), so
    // this pins the harness's own "no fixture" warning specifically.
    expect(warn.mock.calls.map((call: unknown[]) => call[0])).not.toContain(NO_FIXTURE_WARNING)
  })
})
