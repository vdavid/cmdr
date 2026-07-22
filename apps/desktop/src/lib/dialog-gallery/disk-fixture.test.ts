/**
 * The listing handle is the part that fails SILENTLY.
 *
 * `mkdir-confirmation` / `new-file-confirmation` take a pane-owned `listingId`
 * and use it for the conflict lookup, the directory-diff filter, and
 * `refreshListing`. Hand them a made-up one and nothing throws: the conflict
 * check just quietly stops working, which is the "renders broken, wastes the
 * review" outcome the gallery exists to prevent. So: it comes from the pane the
 * gallery navigated, or the preview doesn't open at all.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { resolveDiskFixture, type FixtureDirPayload } from './disk-fixture'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

const getFilesAtIndices = vi.fn(() => Promise.resolve([{ name: 'Photos', path: '/fixtures/Photos' }]))
vi.mock('$lib/tauri-commands', () => ({
  getFilesAtIndices: (...args: unknown[]) => getFilesAtIndices(...(args as [])),
}))

const navigateToDirInPane = vi.fn(() => Promise.resolve(true))
const resolveLocationOrToast = vi.fn(() => Promise.resolve<{ volumeId: string; path: string } | null>(null))
vi.mock('$lib/file-explorer/navigation/navigate-and-select', () => ({
  navigateToDirInPane: (...args: unknown[]) => navigateToDirInPane(...(args as [])),
  resolveLocationOrToast: (...args: unknown[]) => resolveLocationOrToast(...(args as [])),
}))

vi.mock('$lib/file-explorer/pane/explorer-state.svelte', () => ({
  explorerState: { getShowHiddenFiles: () => true, getTabMgr: () => 'tab-manager' },
}))

vi.mock('$lib/file-explorer/tabs/tab-state-manager.svelte', () => ({
  getActiveTab: () => ({ volumeId: 'root', sortBy: 'modified', sortOrder: 'descending' }),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

const fixtures: FixtureDirPayload = {
  root: '/fixtures',
  destinationDir: '/fixtures/Backup destination',
  existingFolderName: 'Photos',
  existingFileName: 'Invoice 2026-07.pdf',
  nestedPath: '/fixtures/Projects/cmdr',
}

function makeExplorer(listingId: string | null): ExplorerAPI {
  return {
    getFocusedPane: () => 'right',
    getPaneListingId: vi.fn(() => listingId),
  } as unknown as ExplorerAPI
}

beforeEach(() => {
  vi.clearAllMocks()
  navigateToDirInPane.mockResolvedValue(true)
  resolveLocationOrToast.mockResolvedValue({ volumeId: 'root', path: '/fixtures' })
  getFilesAtIndices.mockResolvedValue([{ name: 'Photos', path: '/fixtures/Photos' }])
})

describe('resolveDiskFixture', () => {
  it('navigates the focused pane and carries that pane’s real listing id', async () => {
    const explorer = makeExplorer('listing-7')

    const disk = await resolveDiskFixture(explorer, fixtures)

    expect(navigateToDirInPane).toHaveBeenCalledWith(explorer, 'right', { volumeId: 'root', path: '/fixtures' })
    expect(disk).toMatchObject({
      root: '/fixtures',
      destinationDir: '/fixtures/Backup destination',
      paneSide: 'right',
      listingId: 'listing-7',
      volumeId: 'root',
      showHiddenFiles: true,
      sortColumn: 'modified',
      sortOrder: 'descending',
    })
    expect(disk?.entries).toHaveLength(1)
    // Backend indices, so the synthetic `..` row can never reach a fixture.
    expect(getFilesAtIndices).toHaveBeenCalledWith('listing-7', [0, 1, 2, 3, 4, 5], true)
  })

  it('opens nothing when the pane refused to navigate', async () => {
    // The refusal is the dangerous case: the pane keeps its PREVIOUS directory's
    // still-valid listing id, so the `!listingId` guard below sails right past it
    // and the dialog would open against the wrong directory with real-looking
    // entries and tallies. Silently reviewing the wrong folder is exactly what
    // this module exists to prevent.
    navigateToDirInPane.mockResolvedValue(false)

    expect(await resolveDiskFixture(makeExplorer('listing-of-the-previous-dir'), fixtures)).toBeNull()
    expect(getFilesAtIndices).not.toHaveBeenCalled()
  })

  it('opens nothing when the pane has no listing yet', async () => {
    expect(await resolveDiskFixture(makeExplorer(null), fixtures)).toBeNull()
    expect(getFilesAtIndices).not.toHaveBeenCalled()
  })

  it('opens nothing when the fixture directory doesn’t resolve to a volume', async () => {
    resolveLocationOrToast.mockResolvedValue(null)

    expect(await resolveDiskFixture(makeExplorer('listing-7'), fixtures)).toBeNull()
    expect(navigateToDirInPane).not.toHaveBeenCalled()
  })

  it('opens nothing before the explorer has mounted', async () => {
    expect(await resolveDiskFixture(undefined, fixtures)).toBeNull()
  })
})
