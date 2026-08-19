/**
 * The Duplicate command's decisions, with the two listing-driven props builders
 * stubbed so each branch (selection vs cursor) is observable without standing up
 * a listing fixture. What's asserted here is what the command SENDS: the same
 * folder on both sides of the transfer, and `duplicateFollowUp: 'nothing'`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PaneAccess } from './pane-access'
import type { FilePaneAPI } from './types'
import type { VolumeInfo } from '../types'

const { addToastSpy, buildFromSelectionSpy, buildFromCursorSpy } = vi.hoisted(() => ({
  addToastSpy: vi.fn<(content: unknown, options?: unknown) => string>(),
  buildFromSelectionSpy: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
  buildFromCursorSpy: vi.fn<(...args: unknown[]) => Promise<unknown>>(),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: addToastSpy }))

// The destination guard resolves the pane's kind through the capability table,
// which reads the volume store; an empty store lets a real id ('root') fall to
// the listable `local` default. The read-only alert reads `access.getVolumes()`
// instead, which is the fixture below.
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

vi.mock('$lib/search/capabilities', () => ({
  SEARCH_RESULTS_NOT_A_FOLDER_TOAST: "Search results aren't a folder. Pick a real destination.",
}))

vi.mock('./transfer-operations', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./transfer-operations')>()
  return {
    ...actual,
    buildTransferPropsFromSelection: buildFromSelectionSpy,
    buildTransferPropsFromCursor: buildFromCursorSpy,
  }
})

import { duplicateInPlace } from './duplicate-command'

type Dialogs = Parameters<typeof duplicateInPlace>[1]

/** A pane stub exposing only what the Duplicate command reads. */
function paneRef(overrides: { listingId?: string | null; selectedIndices?: number[]; cursorIndex?: number } = {}) {
  return {
    getListingId: () => ('listingId' in overrides ? overrides.listingId : 'lst-1'),
    hasParentEntry: () => false,
    getSelectedIndices: () => overrides.selectedIndices ?? [],
    getCursorIndex: () => overrides.cursorIndex ?? 0,
  } as unknown as FilePaneAPI
}

function access(config: { ref?: FilePaneAPI; path?: string; volumes?: VolumeInfo[] } = {}): PaneAccess {
  return {
    getPaneRef: () => config.ref ?? paneRef(),
    getPanePath: () => config.path ?? '/Users/x/dir',
    getPaneVolumeId: () => 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getFocusedPane: () => 'left',
    otherPane: (pane: 'left' | 'right') => (pane === 'left' ? 'right' : 'left'),
    getShowHiddenFiles: () => true,
    getVolumes: () => config.volumes ?? [volume()],
  } as unknown as PaneAccess
}

function dialogs() {
  return { startTransferProgress: vi.fn(), showAlert: vi.fn() } as unknown as Dialogs & {
    startTransferProgress: ReturnType<typeof vi.fn>
    showAlert: ReturnType<typeof vi.fn>
  }
}

function volume(overrides: Partial<VolumeInfo> = {}): VolumeInfo {
  return { id: 'root', name: 'Macintosh HD', mountIsReadOnly: false, supportsTrash: true, ...overrides } as VolumeInfo
}

/** What the stubbed builders hand back, minus the paths the caller varies. */
function builtProps(sourcePaths: string[]) {
  return { sourcePaths, fileCount: sourcePaths.length, folderCount: 0, sortColumn: 'name', sortOrder: 'ascending' }
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('duplicateInPlace', () => {
  it('copies the selection into the folder it already lives in', async () => {
    buildFromSelectionSpy.mockResolvedValue(builtProps(['/Users/x/dir/a.txt', '/Users/x/dir/b.txt']))
    const d = dialogs()

    await duplicateInPlace(access({ ref: paneRef({ selectedIndices: [0, 1] }) }), d)

    expect(buildFromCursorSpy).not.toHaveBeenCalled()
    // Source and destination are the same folder. That IS the duplicate.
    const context = buildFromSelectionSpy.mock.calls[0]?.[5] as { sourcePath: string; destPath: string }
    expect(context.sourcePath).toBe('/Users/x/dir')
    expect(context.destPath).toBe('/Users/x/dir')
    expect(d.startTransferProgress).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        operationType: 'copy',
        sourcePaths: ['/Users/x/dir/a.txt', '/Users/x/dir/b.txt'],
        sourceFolderPath: '/Users/x/dir',
        destinationPath: '/Users/x/dir',
        // The copy lands in the pane the user is looking at, and its source is
        // that same pane, so both sides name it.
        direction: 'left',
        sourcePaneSide: 'left',
      }),
    )
  })

  it('copies the cursor item when nothing is selected', async () => {
    buildFromCursorSpy.mockResolvedValue(builtProps(['/Users/x/dir/photo.jpg']))
    const d = dialogs()

    await duplicateInPlace(access({ ref: paneRef({ selectedIndices: [], cursorIndex: 3 }) }), d)

    expect(buildFromSelectionSpy).not.toHaveBeenCalled()
    expect(d.startTransferProgress).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ sourcePaths: ['/Users/x/dir/photo.jpg'], destinationPath: '/Users/x/dir' }),
    )
  })

  it('never opens the rename editor on the copy', async () => {
    // ⌘D is Finder's Duplicate, and the familiarity that justifies the key rests
    // on it asking nothing. Paste and F5 are the gestures that open the editor.
    buildFromCursorSpy.mockResolvedValue(builtProps(['/Users/x/dir/photo.jpg']))
    const d = dialogs()

    await duplicateInPlace(access(), d)

    expect(d.startTransferProgress).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ duplicateFollowUp: 'nothing' }),
    )
  })

  it('refuses on a read-only volume with the shared alert and starts nothing', async () => {
    const d = dialogs()

    await duplicateInPlace(access({ volumes: [volume({ mountIsReadOnly: true })] }), d)

    expect(d.showAlert).toHaveBeenCalledWith(
      'Read-only device',
      '"Macintosh HD" is read-only. You can copy files from it, but not to it.',
    )
    expect(d.startTransferProgress).not.toHaveBeenCalled()
  })

  it('starts nothing when the pane has no listing to read', async () => {
    const d = dialogs()

    await duplicateInPlace(access({ ref: paneRef({ listingId: null }) }), d)

    expect(d.startTransferProgress).not.toHaveBeenCalled()
  })

  it('starts nothing when there is nothing under the cursor', async () => {
    // A `..` row or an empty listing: the builder answers null and the command
    // simply doesn't dispatch.
    buildFromCursorSpy.mockResolvedValue(null)
    const d = dialogs()

    await duplicateInPlace(access(), d)

    expect(d.startTransferProgress).not.toHaveBeenCalled()
  })
})
