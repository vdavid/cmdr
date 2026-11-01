// @vitest-environment jsdom
/**
 * What a screen reader hears when a date sort falls back to Modified, through
 * `@guidepup/virtual-screen-reader`: the real sort path (`createSortOperations`) raising
 * its toast into the real toast stack. The toast's live region is the ONE announcement
 * (no `announce` beside it), and an order flip of the same fallback stays quiet, so the
 * sentence is spoken exactly once. It models the ARIA spec over jsdom, not VoiceOver;
 * David's listening check covers the real thing.
 */

import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import { virtual } from '@guidepup/virtual-screen-reader'
import ToastContainer from '#lib/ui/toast/ToastContainer.svelte'
import { clearAllToasts } from '#lib/ui/toast/index.js'
import { pastPendingFrames } from '#lib/test-frames.js'
import { untilSpoken } from '#lib/test-vsr.js'
import type { ResortResult, SortColumn, SortOrder } from '../types'
import type { FilePaneAPI } from './types'
import { createPaneRowState } from './pane-row-state'

const { resortListingSpy } = vi.hoisted(() => ({
  resortListingSpy: vi.fn<(...args: unknown[]) => Promise<ResortResult>>(),
}))

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/tauri-commands/index.js')>()),
  resortListing: resortListingSpy,
}))

import { createSortOperations } from './sort-operations'

const ADDED_FALLBACK = 'This location doesn’t record when items were added, so Cmdr sorts by date modified here.'

let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  clearAllToasts()
  // The location honors no date: every sort lands in Modified.
  resortListingSpy.mockResolvedValue({
    newCursorIndex: null,
    newSelectedIndices: null,
    sequence: 1,
    totalCount: 3,
    effectiveSortBy: 'modified',
  })
})

afterEach(async () => {
  if (component) void unmount(component)
  component = null
  clearAllToasts()
  await virtual.stop()
  document.body.innerHTML = ''
})

/** A folder pane, holding only what the sort path touches. */
function folderPane(): FilePaneAPI {
  const rows = createPaneRowState({
    getListingId: () => 'listing-1',
    getLoading: () => false,
    getOperationActive: () => false,
    getIncludeHidden: () => false,
  })
  rows.initialize()
  const ref = {
    cancelRename: () => {},
    clearJumpState: () => {},
    getListingId: () => 'listing-1',
    getCurrentPath: () => '/Volumes/USB',
    getFilenameUnderCursor: () => 'a.txt',
    getSelectedIndices: () => [],
    isAllSelected: () => false,
    hasParentEntry: () => false,
    getRowState: () => rows,
    applyRowResult: () => ({}),
  }
  return ref as unknown as FilePaneAPI
}

describe('a date sort that falls back, spoken', () => {
  it('says why once, and an order flip of the same sort says nothing more', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    component = mount(ToastContainer, { target })
    await tick()
    await virtual.start({ container: document.body })

    const pane = folderPane()
    const sort: { sortBy: SortColumn; sortOrder: SortOrder } = { sortBy: 'name', sortOrder: 'ascending' }
    const ops = createSortOperations({
      getPaneRef: () => pane,
      getPaneSort: () => sort,
      setPaneSort: (_pane, sortBy, sortOrder) => {
        sort.sortBy = sortBy
        sort.sortOrder = sortOrder
      },
      getShowHiddenFiles: () => false,
      getFocusedPane: () => 'left',
    })

    await ops.handleSortChange('left', 'added')
    await untilSpoken(`polite: ${ADDED_FALLBACK}`)
    await ops.handleSortChange('left', 'added')
    await pastPendingFrames()

    expect(resortListingSpy).toHaveBeenCalledTimes(2)
    const log = await virtual.spokenPhraseLog()
    expect(log.filter((phrase) => phrase.includes(ADDED_FALLBACK))).toHaveLength(1)
  })
})
