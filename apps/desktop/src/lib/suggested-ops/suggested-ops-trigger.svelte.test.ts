/**
 * The Suggested ops state slice: what the dialog reads, what it holds while the user decides,
 * and the three behaviours the design leans on — ops arriving a window at a time, deselection
 * surviving a scroll, and a group the agent changed being ANNOUNCED rather than swapped under
 * the reader.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { SuggestedOpPage, SuggestedSweepView } from '$lib/tauri-commands'

const listMock = vi.fn<() => Promise<SuggestedSweepView[]>>()
const pageMock = vi.fn<(g: number, o: number, l: number) => Promise<SuggestedOpPage>>()
const rejectMock = vi.fn<(g: number) => Promise<{ kind: string }>>()

vi.mock('$lib/tauri-commands', () => ({
  listSuggestedOps: () => listMock(),
  pageSuggestedOps: (g: number, o: number, l: number) => pageMock(g, o, l),
  rejectSuggestedGroup: (g: number) => rejectMock(g),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import {
  approvableCount,
  closeSuggestedOps,
  collapseGroup,
  ensureOpWindow,
  expandGroup,
  opAt,
  openGroup,
  openSuggestedOps,
  refreshSuggestions,
  rejectGroup,
  suggestedOpsState,
  toggleOp,
} from './suggested-ops-trigger.svelte'

function group(groupId: number, liveOpCount: number, overrides: Record<string, unknown> = {}) {
  return {
    groupId,
    sweepId: 1,
    verb: 'move',
    status: 'pending',
    displayName: 'five invoices',
    rationale: 'They all look like invoices.',
    sourceVolumeId: 'root',
    destination: '/Users/someone/Documents/Invoices',
    reversible: 'restoreMove',
    destinationState: 'willBeCreated',
    liveOpCount,
    totalOpCount: liveOpCount,
    fromSelector: false,
    ...overrides,
  } as unknown as SuggestedSweepView['groups'][number]
}

function sweep(groups: SuggestedSweepView['groups']): SuggestedSweepView {
  return { sweepId: 1, createdAt: 100, rationale: 'Ten new files in Downloads.', groups }
}

function ops(from: number, count: number) {
  return Array.from({ length: count }, (_, i) => ({
    opId: from + i,
    sourcePath: `/Users/someone/Downloads/file-${String(from + i)}.dmg`,
    newName: null,
    status: 'pending',
    snapshotSize: 1024,
    snapshotModified: 1_780_000_000,
  }))
}

beforeEach(() => {
  vi.clearAllMocks()
  closeSuggestedOps()
  suggestedOpsState.sweeps = []
  suggestedOpsState.loadError = false
  listMock.mockResolvedValue([sweep([group(7, 3)])])
  pageMock.mockImplementation((_g, offset) =>
    Promise.resolve({ ops: ops(offset, 200), offset, total: 60_000 } as unknown as SuggestedOpPage),
  )
  rejectMock.mockResolvedValue({ kind: 'rejected' })
})

describe('opening and reading', () => {
  it('opens once however many times the menu, palette, and shortcut fire', async () => {
    await openSuggestedOps()
    await openSuggestedOps()

    expect(suggestedOpsState.open).toBe(true)
    expect(listMock).toHaveBeenCalledTimes(1)
  })

  it('opens even when the read throws, and says so instead of showing an empty list', async () => {
    listMock.mockRejectedValueOnce(new Error('no store'))

    await openSuggestedOps()

    expect(suggestedOpsState.open).toBe(true)
    expect(suggestedOpsState.loadError).toBe(true)
    expect(suggestedOpsState.sweeps).toEqual([])
  })
})

describe('the op window', () => {
  it('never loads a whole group to show it', async () => {
    await openSuggestedOps()
    await expandGroup(7)

    // 60,000 ops, one window fetched.
    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(pageMock.mock.calls[0]?.[2]).toBeLessThanOrEqual(200)
    expect(suggestedOpsState.window?.total).toBe(60_000)
    expect(suggestedOpsState.window?.ops.length).toBe(200)
  })

  it('reads rows already held without another round trip', async () => {
    await openSuggestedOps()
    await expandGroup(7)
    pageMock.mockClear()

    await ensureOpWindow(7, 10)

    expect(pageMock).not.toHaveBeenCalled()
    expect(opAt(10)?.opId).toBe(10)
  })

  it('fetches a new window when the viewport jumps past the one it holds', async () => {
    await openSuggestedOps()
    await expandGroup(7)
    pageMock.mockClear()

    await ensureOpWindow(7, 5_000)

    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(opAt(5_000)).not.toBeNull()
  })

  it('answers null for a row whose window is not loaded, rather than a wrong row', async () => {
    await openSuggestedOps()
    await expandGroup(7)

    expect(opAt(50_000)).toBeNull()
  })
})

describe('deselection', () => {
  it('survives the row scrolling out of the loaded window', async () => {
    await openSuggestedOps()
    await expandGroup(7)
    toggleOp(3)

    await ensureOpWindow(7, 5_000)

    expect(opAt(3)).toBeNull()
    expect(suggestedOpsState.deselected.has(3)).toBe(true)
  })

  it('counts what would actually run from the COUNT, not from the rows in memory', async () => {
    listMock.mockResolvedValue([sweep([group(7, 60_000)])])
    await openSuggestedOps()
    await expandGroup(7)

    toggleOp(1)
    toggleOp(2)

    expect(approvableCount()).toBe(59_998)
  })

  it('turns a row back on', async () => {
    await openSuggestedOps()
    await expandGroup(7)

    toggleOp(1)
    toggleOp(1)

    expect(suggestedOpsState.deselected.has(1)).toBe(false)
  })

  it('starts a different group with a clean slate', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    await expandGroup(7)
    toggleOp(1)

    await expandGroup(8)

    expect(suggestedOpsState.deselected.size).toBe(0)
  })
})

describe('a group that changed under the review', () => {
  it('announces the change and leaves the rows exactly where they are', async () => {
    await openSuggestedOps()
    await expandGroup(7)
    const before = suggestedOpsState.window?.ops[0]?.opId

    // The agent amended the group: same id, different op count.
    listMock.mockResolvedValueOnce([sweep([group(7, 9)])])
    await refreshSuggestions()

    expect(suggestedOpsState.changedUnderReview).toBe(true)
    expect(suggestedOpsState.openGroupId).toBe(7)
    expect(suggestedOpsState.window?.ops[0]?.opId).toBe(before)
  })

  it('stays quiet when nothing about the open group moved', async () => {
    await openSuggestedOps()
    await expandGroup(7)

    await refreshSuggestions()

    expect(suggestedOpsState.changedUnderReview).toBe(false)
  })

  it('collapses when the group the user was reading is gone', async () => {
    await openSuggestedOps()
    await expandGroup(7)

    listMock.mockResolvedValueOnce([])
    await refreshSuggestions()

    expect(suggestedOpsState.openGroupId).toBeNull()
    expect(suggestedOpsState.window).toBeNull()
  })
})

describe('rejecting', () => {
  it('records the no, collapses the group, and re-reads', async () => {
    await openSuggestedOps()
    await expandGroup(7)
    listMock.mockResolvedValueOnce([])

    await rejectGroup(7)

    expect(rejectMock).toHaveBeenCalledWith(7)
    expect(suggestedOpsState.openGroupId).toBeNull()
    expect(suggestedOpsState.sweeps).toEqual([])
    expect(suggestedOpsState.busyGroupId).toBeNull()
  })

  it('re-reads rather than insisting when somebody already answered the group', async () => {
    rejectMock.mockResolvedValueOnce({ kind: 'alreadyAnswered' })
    await openSuggestedOps()

    await rejectGroup(7)

    expect(listMock).toHaveBeenCalledTimes(2)
    expect(suggestedOpsState.loadError).toBe(false)
  })

  it('leaves the dialog usable when the rejection throws', async () => {
    rejectMock.mockRejectedValueOnce(new Error('gone'))
    await openSuggestedOps()

    await rejectGroup(7)

    expect(suggestedOpsState.busyGroupId).toBeNull()
    expect(suggestedOpsState.open).toBe(true)
  })
})

describe('closing', () => {
  it('drops the open group and its deselection', async () => {
    await openSuggestedOps()
    await expandGroup(7)
    toggleOp(1)

    closeSuggestedOps()

    expect(suggestedOpsState.open).toBe(false)
    expect(suggestedOpsState.openGroupId).toBeNull()
    expect(suggestedOpsState.deselected.size).toBe(0)
  })

  it('reports the open group header while one is expanded', async () => {
    await openSuggestedOps()
    await expandGroup(7)

    expect(openGroup()?.groupId).toBe(7)

    collapseGroup()

    expect(openGroup()).toBeNull()
  })
})
