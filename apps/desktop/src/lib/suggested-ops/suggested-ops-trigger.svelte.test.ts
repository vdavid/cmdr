/**
 * The AI suggestions state slice: what the dialog reads, what it holds while the user decides,
 * and the behaviours the design leans on — every group's list showing at once, each paging a
 * window at a time, deselection surviving a scroll, live facts read only for drawn rows, and a
 * group the agent changed being ANNOUNCED rather than swapped under the reader.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type {
  ServerApprovalResultView,
  SuggestedOpPage,
  SuggestedSweepView,
  SuggestionsChanged,
} from '#lib/tauri-commands/index.js'

const listMock = vi.fn<() => Promise<SuggestedSweepView[]>>()
const pageMock = vi.fn<(payload: { groupId: number; offset: number; limit: number }) => Promise<SuggestedOpPage>>()
const rejectMock = vi.fn<(g: number) => Promise<{ kind: string }>>()
const approveMock =
  vi.fn<
    (payload: { groupId: number; revision: number; selection: Record<string, unknown> }) => Promise<{ kind: string }>
  >()
const runsAsMock = vi.fn<(runsAs: string | null) => void>()
const factsMock = vi.fn<(payload: { groupId: number; offset: number; limit: number }) => Promise<unknown>>()
const approveServerMock = vi.fn<(payload: { groupId: number; revision: number }) => Promise<ServerApprovalResultView>>()

/** The subscriber the store installs while the dialog is open. */
let emit: ((payload: SuggestionsChanged) => void) | null = null

vi.mock('#lib/tauri-commands/index.js', () => ({
  listSuggestedOps: () => listMock(),
  pageSuggestedOps: (g: number, o: number, l: number) => pageMock({ groupId: g, offset: o, limit: l }),
  rejectSuggestedGroup: (g: number) => rejectMock(g),
  approveSuggestedGroup: (g: number, revision: number, selection: Record<string, unknown>, runsAs: string | null) => {
    runsAsMock(runsAs)
    return approveMock({ groupId: g, revision, selection })
  },
  approveSuggestedServer: (g: number, revision: number) => approveServerMock({ groupId: g, revision }),
  liveSuggestedOpFacts: (g: number, o: number, l: number) => factsMock({ groupId: g, offset: o, limit: l }),
  onSuggestionsChanged: (cb: (p: SuggestionsChanged) => void) => {
    emit = cb
    return Promise.resolve(() => {
      emit = null
    })
  },
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { SuggestedOpsFailure } from './suggested-ops-failure'
import {
  INITIAL_ROWS,
  SHOW_MORE_STEP,
  approvableCount,
  canApprove,
  approveGroup,
  isNewNameTaken,
  isOpMissing,
  isOpSelected,
  selectionState,
  toggleAllOps,
  closeSuggestedOps,
  ensureOpWindow,
  opAt,
  openSuggestedOps,
  refreshSuggestions,
  rejectGroup,
  reloadGroup,
  reviewOf,
  showAllRows,
  showMoreRows,
  showRange,
  shownRows,
  suggestedOpsState,
  toggleOp,
} from './suggested-ops-trigger.svelte'

/** An op row as the toggles see it: an id, and whether an earlier review turned it off. */
function pending(opId: number) {
  return {
    opId,
    sourcePath: `/x/${String(opId)}`,
    newName: null,
    status: 'pending',
    snapshotSize: null,
    snapshotModified: null,
  } as const
}
function excluded(opId: number) {
  return { ...pending(opId), status: 'excluded' } as const
}

function group(groupId: number, liveOpCount: number, overrides: Record<string, unknown> = {}) {
  return {
    groupId,
    sweepId: 1,
    verb: 'move',
    status: 'pending',
    displayName: 'five invoices',
    rationale: 'They all look like invoices.',
    sourceVolumeId: 'root',
    sourceFolder: '/Users/someone/Downloads',
    destination: '/Users/someone/Documents/Invoices',
    destinationVolumeName: 'Macintosh HD',
    reversible: 'restoreMove',
    destinationState: 'willBeCreated',
    liveOpCount,
    totalOpCount: liveOpCount,
    fromSelector: false,
    server: null,
    revision: 1,
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

/** The group the default sweep holds: 60,000 ops, which is the size nothing may load whole. */
const BIG = 7

beforeEach(() => {
  vi.clearAllMocks()
  closeSuggestedOps()
  suggestedOpsState.sweeps = []
  suggestedOpsState.loadError = false
  listMock.mockResolvedValue([sweep([group(BIG, 60_000)])])
  pageMock.mockImplementation(({ offset }) =>
    Promise.resolve({ ops: ops(offset, 200), offset, total: 60_000 } as unknown as SuggestedOpPage),
  )
  factsMock.mockImplementation(({ offset, limit }) =>
    Promise.resolve({
      kind: 'facts',
      facts: Array.from({ length: limit }, (_, i) => ({
        opId: offset + i,
        state: { kind: 'present', size: 2048, modified: 1_790_000_000 },
      })),
    }),
  )
  rejectMock.mockResolvedValue({ kind: 'rejected' })
  approveMock.mockResolvedValue({ kind: 'started' })
  approveServerMock.mockResolvedValue({ kind: 'saved', volumeId: null })
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

  it('shows every group’s files right away, one page each', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])

    await openSuggestedOps()

    expect(pageMock.mock.calls.map(([call]) => call.groupId).sort()).toEqual([7, 8])
    expect(opAt(7, 0)).not.toBeNull()
    expect(opAt(8, 0)).not.toBeNull()
  })
})

describe('the op window', () => {
  it('never loads a whole group to show it', async () => {
    await openSuggestedOps()

    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(pageMock.mock.calls[0]?.[0]?.limit).toBeLessThanOrEqual(200)
    expect(reviewOf(BIG)?.window?.total).toBe(60_000)
    expect(reviewOf(BIG)?.window?.ops.length).toBe(200)
  })

  it('reads rows already held without another round trip', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    await ensureOpWindow(BIG, 10)

    expect(pageMock).not.toHaveBeenCalled()
    expect(opAt(BIG, 10)?.opId).toBe(10)
  })

  it('fetches a new window when the list jumps past the one it holds', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    await ensureOpWindow(BIG, 5_000)

    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(opAt(BIG, 5_000)).not.toBeNull()
  })

  it('follows a jump made while a window was still loading, instead of leaving its rows blank', async () => {
    await openSuggestedOps()
    pageMock.mockClear()
    let land: () => void = () => {}
    pageMock.mockImplementationOnce(
      ({ offset }) =>
        new Promise((resolve) => {
          land = () => {
            resolve({ ops: ops(offset, 200), offset, total: 60_000 } as unknown as SuggestedOpPage)
          }
        }),
    )

    const first = showRange(BIG, 5_000, 5_010)
    // The user drags on before that page lands; the list won't ask again for a range it already sent.
    const second = showRange(BIG, 40_000, 40_010)
    land()
    await Promise.all([first, second])

    expect(opAt(BIG, 40_000)).not.toBeNull()
  })

  // Regression: only the range's FIRST row was checked, so scrolling the drawn range across
  // the held window's end left its bottom rows as blank placeholders until the top crossed too.
  it('holds every drawn row when the range runs past the end of the held window', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    await showRange(BIG, 180, 240)

    expect(pageMock).toHaveBeenCalledTimes(1)
    for (let index = 180; index < 240; index++) expect(opAt(BIG, index)?.opId).toBe(index)
  })

  it('follows a range that grew past the window while a page was loading', async () => {
    await openSuggestedOps()
    pageMock.mockClear()
    let land: () => void = () => {}
    pageMock.mockImplementationOnce(
      ({ offset }) =>
        new Promise((resolve) => {
          land = () => {
            resolve({ ops: ops(offset, 200), offset, total: 60_000 } as unknown as SuggestedOpPage)
          }
        }),
    )

    const first = showRange(BIG, 5_000, 5_060)
    // Same first row, but the range now reaches past what the in-flight page will hold.
    const second = showRange(BIG, 5_000, 5_190)
    land()
    await Promise.all([first, second])

    expect(opAt(BIG, 5_189)).not.toBeNull()
  })

  it('answers null for a row whose window is not loaded, rather than a wrong row', async () => {
    await openSuggestedOps()

    expect(opAt(BIG, 50_000)).toBeNull()
  })

  it('keeps each group’s window to itself', async () => {
    listMock.mockResolvedValue([sweep([group(7, 60_000), group(8, 60_000)])])
    await openSuggestedOps()

    await ensureOpWindow(8, 5_000)

    expect(opAt(7, 0)).not.toBeNull()
    expect(opAt(8, 0)).toBeNull()
    expect(opAt(8, 5_000)).not.toBeNull()
  })
})

describe('how many rows a list shows', () => {
  it('starts at the first few, then grows on request', async () => {
    await openSuggestedOps()
    const big = suggestedOpsState.sweeps[0].groups[0]

    expect(shownRows(big)).toBe(INITIAL_ROWS)
    showMoreRows(BIG)
    expect(shownRows(big)).toBe(INITIAL_ROWS + SHOW_MORE_STEP)
    showAllRows(BIG)
    expect(shownRows(big)).toBe(60_000)
  })

  it('never shows more rows than the group has', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3)])])
    pageMock.mockResolvedValue({ ops: ops(0, 3), offset: 0, total: 3 } as unknown as SuggestedOpPage)
    await openSuggestedOps()

    showMoreRows(7)

    expect(shownRows(suggestedOpsState.sweeps[0].groups[0])).toBe(3)
  })

  it('“Show all” on 60,000 still reads one window, not the group', async () => {
    await openSuggestedOps()
    pageMock.mockClear()

    showAllRows(BIG)

    expect(pageMock).not.toHaveBeenCalled()
  })
})

describe('live facts', () => {
  it('checks only the rows the list drew, once each', async () => {
    await openSuggestedOps()

    expect(factsMock).toHaveBeenCalledTimes(1)
    expect(factsMock).toHaveBeenCalledWith({ groupId: BIG, offset: 0, limit: INITIAL_ROWS })

    factsMock.mockClear()
    await showRange(BIG, 0, INITIAL_ROWS)

    expect(factsMock).not.toHaveBeenCalled()
  })

  it('checks newly drawn rows as the list scrolls', async () => {
    await openSuggestedOps()
    factsMock.mockClear()

    await showRange(BIG, 5, 15)

    expect(factsMock).toHaveBeenCalledWith({ groupId: BIG, offset: 10, limit: 5 })
    expect(reviewOf(BIG)?.liveFacts.get(12)).toEqual({ kind: 'present', size: 2048, modified: 1_790_000_000 })
  })

  it('stops asking a source that isn’t connected, and says so once for the group', async () => {
    factsMock.mockResolvedValueOnce({ kind: 'refused', reason: { type: 'sourceNotConnected' } })
    await openSuggestedOps()
    factsMock.mockClear()

    await showRange(BIG, 5, 25)

    expect(reviewOf(BIG)?.sourceUnreachable).toBe('notConnected')
    expect(factsMock).not.toHaveBeenCalled()
  })

  it('asks again after a reload, in case the source came back', async () => {
    factsMock.mockResolvedValueOnce({ kind: 'refused', reason: { type: 'sourceGone' } })
    await openSuggestedOps()

    await reloadGroup(BIG)

    expect(reviewOf(BIG)?.sourceUnreachable).toBeNull()
    expect(reviewOf(BIG)?.liveFacts.size).toBe(INITIAL_ROWS)
  })

  it('asks again on the next scroll when the store refused', async () => {
    factsMock.mockResolvedValueOnce({ kind: 'refused', reason: { type: 'store', detail: 'database is locked' } })
    await openSuggestedOps()
    expect(reviewOf(BIG)?.liveFacts.size).toBe(0)

    await showRange(BIG, 0, INITIAL_ROWS)

    expect(reviewOf(BIG)?.liveFacts.size).toBe(INITIAL_ROWS)
  })

  it('asks again on the next scroll when a check threw', async () => {
    factsMock.mockRejectedValueOnce(new Error('share went away'))
    await openSuggestedOps()
    expect(reviewOf(BIG)?.liveFacts.size).toBe(0)

    await showRange(BIG, 0, INITIAL_ROWS)

    expect(reviewOf(BIG)?.liveFacts.size).toBe(INITIAL_ROWS)
  })
})

describe('approving', () => {
  it('sends the ids the user turned OFF, never the ones they kept', async () => {
    await openSuggestedOps()
    toggleOp(BIG, pending(4))
    toggleOp(BIG, pending(9))

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith({
      groupId: BIG,
      revision: 1,
      selection: { mode: 'fromStored', excludeOpIds: [4, 9], includeOpIds: [] },
    })
  })

  it('carries an empty list when the whole group is approved, at any size', async () => {
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith({
      groupId: BIG,
      revision: 1,
      selection: { mode: 'fromStored', excludeOpIds: [], includeOpIds: [] },
    })
  })

  it('sends only that group’s deselection', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    toggleOp(8, pending(1))

    await approveGroup(7)

    expect(approveMock).toHaveBeenCalledWith({
      groupId: 7,
      revision: 1,
      selection: { mode: 'fromStored', excludeOpIds: [], includeOpIds: [] },
    })
  })

  it('closes the dialog once nothing is left to decide', async () => {
    await openSuggestedOps()
    listMock.mockResolvedValue([])

    await approveGroup(BIG)

    expect(suggestedOpsState.open).toBe(false)
    expect(suggestedOpsState.reviews.size).toBe(0)
  })

  describe('a server card', () => {
    const SERVER = 9
    const card = { protocol: 'sftp', fields: [], addressIsNew: true, sharesAffected: null }

    beforeEach(() => {
      listMock.mockResolvedValue([
        sweep([group(SERVER, 0, { verb: 'addServer', destination: null, server: card, revision: 2 })]),
      ])
    })

    it('saves through its own command with the revision the user read, and selects no ops', async () => {
      await openSuggestedOps()
      expect(canApprove(SERVER)).toBe(true)

      await approveGroup(SERVER)

      expect(approveServerMock).toHaveBeenCalledWith({ groupId: SERVER, revision: 2 })
      expect(approveMock).not.toHaveBeenCalled()
    })

    it('closes the dialog once a save leaves nothing to decide', async () => {
      await openSuggestedOps()
      listMock.mockResolvedValue([])

      await approveGroup(SERVER)

      expect(suggestedOpsState.open).toBe(false)
    })

    it('raises the changed notice when the card was amended since it was read', async () => {
      approveServerMock.mockResolvedValueOnce({ kind: 'listChanged' })
      await openSuggestedOps()

      await approveGroup(SERVER)

      expect(reviewOf(SERVER)?.changedUnderReview).toBe(true)
      expect(canApprove(SERVER)).toBe(false)
    })

    it('stays on the list when the save is refused', async () => {
      approveServerMock.mockResolvedValueOnce({ kind: 'nameTaken', name: 'Naspolya' })
      await openSuggestedOps()

      await approveGroup(SERVER)

      expect(suggestedOpsState.open).toBe(true)
      expect(listMock).toHaveBeenCalledTimes(2)
    })

    // Zero ops: nothing to page, nothing to check on disk, so nothing asks.
    it('reads no op page for a card, on opening or on a reload', async () => {
      await openSuggestedOps()
      await reloadGroup(SERVER)

      expect(pageMock).not.toHaveBeenCalled()
      expect(factsMock).not.toHaveBeenCalled()
      expect(reviewOf(SERVER)?.window).toBeNull()
    })

    it('keeps every refusal that gave the card back under it, with its reason', async () => {
      const givenBack: ServerApprovalResultView[] = [
        { kind: 'refused', outcome: { outcome: 'address_taken', name: 'Naspolya' } },
        { kind: 'addServer', error: { type: 'invalid_address', message: 'no host' } },
        { kind: 'serverChanged' },
        { kind: 'alreadySaved', serverId: 'sftp-build-22-ada' },
        { kind: 'nameTaken', name: 'Naspolya' },
      ]
      for (const answer of givenBack) {
        approveServerMock.mockResolvedValueOnce(answer)
        await openSuggestedOps()

        await approveGroup(SERVER)

        expect(suggestedOpsState.refusals.get(SERVER)).toEqual({ kind: 'server', answer })
        closeSuggestedOps()
      }
    })

    it('says nothing under a card that saved, was answered elsewhere, or changed', async () => {
      const quiet: ServerApprovalResultView[] = [
        { kind: 'saved', volumeId: 'sftp-build-22-ada' },
        { kind: 'alreadyAnswered' },
        { kind: 'unknown' },
        { kind: 'listChanged' },
      ]
      for (const answer of quiet) {
        approveServerMock.mockResolvedValueOnce(answer)
        await openSuggestedOps()

        await approveGroup(SERVER)

        expect(suggestedOpsState.refusals.has(SERVER)).toBe(false)
        closeSuggestedOps()
      }
    })

    // The saved server moved under the card: the re-read shows the card as it is now.
    it('re-reads the card after the server changed under it', async () => {
      approveServerMock.mockResolvedValueOnce({ kind: 'serverChanged' })
      await openSuggestedOps()

      await approveGroup(SERVER)

      expect(listMock).toHaveBeenCalledTimes(2)
    })
  })

  it('stays open when other suggestions are still waiting', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    listMock.mockResolvedValue([sweep([group(8, 3)])])

    await approveGroup(7)

    expect(suggestedOpsState.open).toBe(true)
    expect(reviewOf(7)).toBeUndefined()
    expect(reviewOf(8)).toBeDefined()
  })

  it('raises the changed notice when the op set moved under the approval', async () => {
    approveMock.mockResolvedValueOnce({ kind: 'listChanged' })
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(reviewOf(BIG)?.changedUnderReview).toBe(true)
    expect(suggestedOpsState.open).toBe(true)
  })

  it('runs nothing for a group the agent changed until the user reads the new version', async () => {
    await openSuggestedOps()
    emit?.({ reason: 'amended', groupId: BIG } as SuggestionsChanged)

    expect(canApprove(BIG)).toBe(false)
    await approveGroup(BIG)
    expect(approveMock).not.toHaveBeenCalled()

    await reloadGroup(BIG)
    expect(canApprove(BIG)).toBe(true)
  })

  it('reopens with the earlier "no"s still off, and counts from the stored live set', async () => {
    // An earlier approval excluded 3 ops, then the engine refused to start: the group is
    // pending again with 3 `excluded` rows, and the new review keeps them unticked.
    listMock.mockResolvedValue([sweep([group(BIG, 7, { totalOpCount: 10 })])])
    await openSuggestedOps()

    expect(approvableCount(BIG)).toBe(7)
    expect(selectionState(BIG)).toBe('some')
    expect(isOpSelected(BIG, excluded(3))).toBe(false)
    expect(isOpSelected(BIG, pending(4))).toBe(true)
  })

  it('ticks an earlier "no" back on as a flip against the stored ticks, never naming the rest', async () => {
    listMock.mockResolvedValue([sweep([group(BIG, 7, { totalOpCount: 10 })])])
    await openSuggestedOps()

    toggleOp(BIG, excluded(3))
    toggleOp(BIG, pending(4))

    expect(isOpSelected(BIG, excluded(3))).toBe(true)
    expect(approvableCount(BIG)).toBe(7)
    await approveGroup(BIG)
    expect(approveMock).toHaveBeenCalledWith({
      groupId: BIG,
      revision: 1,
      selection: { mode: 'fromStored', excludeOpIds: [4], includeOpIds: [3] },
    })
  })

  it('leaves the dialog usable when approving throws', async () => {
    approveMock.mockRejectedValueOnce(new Error('gone'))
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(suggestedOpsState.busyGroupId).toBeNull()
    expect(suggestedOpsState.open).toBe(true)
  })
})

describe('an approval that refuses to start', () => {
  const notConnected = { kind: 'refused', error: { type: 'source_not_connected', path: '/DCIM/one.jpg' } }

  it('keeps the group on the list with the reason under it', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(suggestedOpsState.open).toBe(true)
    expect(suggestedOpsState.sweeps[0].groups.map((g) => g.groupId)).toEqual([BIG])
    expect(suggestedOpsState.refusals.get(BIG)).toEqual(notConnected)
    expect(reviewOf(BIG)).toBeDefined()
  })

  it('gives every refusal to start a reason, not only a volume one', async () => {
    for (const result of [{ kind: 'nothingToRun' }, { kind: 'nothingSelected' }, { kind: 'couldNotStart' }]) {
      approveMock.mockResolvedValueOnce(result)
      await openSuggestedOps()

      await approveGroup(BIG)

      expect(suggestedOpsState.refusals.get(BIG)).toEqual(result)
      closeSuggestedOps()
    }
  })

  it('takes the reason down once the group is approved again', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()
    await approveGroup(BIG)
    listMock.mockResolvedValue([sweep([group(BIG, 3), group(8, 2)])])

    await approveGroup(BIG)

    expect(suggestedOpsState.refusals.has(BIG)).toBe(false)
  })

  it('takes the reason down when the group is rejected', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()
    await approveGroup(BIG)

    await rejectGroup(BIG)

    expect(suggestedOpsState.refusals.has(BIG)).toBe(false)
  })

  it('forgets the reasons by the next time the dialog opens', async () => {
    approveMock.mockResolvedValueOnce(notConnected)
    await openSuggestedOps()
    await approveGroup(BIG)

    closeSuggestedOps()
    await openSuggestedOps()

    expect(suggestedOpsState.refusals.size).toBe(0)
  })

  it('says nothing under a group somebody else already answered', async () => {
    approveMock.mockResolvedValueOnce({ kind: 'alreadyAnswered' })
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(suggestedOpsState.refusals.size).toBe(0)
  })
})

describe('selection', () => {
  it('survives the row scrolling out of the loaded window', async () => {
    await openSuggestedOps()
    toggleOp(BIG, pending(3))

    await ensureOpWindow(BIG, 5_000)

    expect(opAt(BIG, 3)).toBeNull()
    expect(isOpSelected(BIG, pending(3))).toBe(false)
  })

  it('counts what would actually run from the COUNT, not from the rows in memory', async () => {
    await openSuggestedOps()

    toggleOp(BIG, pending(1))
    toggleOp(BIG, pending(2))

    expect(approvableCount(BIG)).toBe(59_998)
  })

  it('turns a row back on', async () => {
    await openSuggestedOps()

    toggleOp(BIG, pending(1))
    toggleOp(BIG, pending(1))

    expect(isOpSelected(BIG, pending(1))).toBe(true)
  })

  it('keeps each group’s selection to itself', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()

    toggleOp(7, pending(1))

    expect(approvableCount(7)).toBe(2)
    expect(approvableCount(8)).toBe(3)
  })

  it('unticks everything from the header with an empty "only" set, at any size', async () => {
    await openSuggestedOps()

    toggleAllOps(BIG)

    expect(selectionState(BIG)).toBe('none')
    expect(approvableCount(BIG)).toBe(0)
    expect(reviewOf(BIG)?.selection).toMatchObject({ mode: 'only' })
  })

  it('sends only the rows ticked by hand after unticking everything', async () => {
    await openSuggestedOps()
    toggleAllOps(BIG)
    toggleOp(BIG, pending(5))
    toggleOp(BIG, pending(6))

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith({ groupId: BIG, revision: 1, selection: { mode: 'only', opIds: [5, 6] } })
  })

  it('reads a mix as mixed, and ticks everything from there', async () => {
    await openSuggestedOps()
    toggleOp(BIG, pending(1))
    expect(selectionState(BIG)).toBe('some')

    toggleAllOps(BIG)

    expect(selectionState(BIG)).toBe('all')
    expect(isOpSelected(BIG, pending(1))).toBe(true)
  })
})

describe('being told a group changed', () => {
  /**
   * `amended` and `approved` carry the SAME group id and mean opposite things: one says the
   * list under the reader moved, the other says the review is over because they ended it.
   * Keying on the id alone raises "this changed" on the user's own approval, which reads as a
   * glitch rather than a bug and would be found late.
   */
  it('raises the notice for an amendment to a group on screen', async () => {
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 1, pendingOpCount: 3, groupId: BIG, reason: 'amended' })

    expect(reviewOf(BIG)?.changedUnderReview).toBe(true)
  })

  it("stays quiet on the user's own approval of the group", async () => {
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 0, pendingOpCount: 0, groupId: BIG, reason: 'approved' })

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('raises it only on the group that was amended', async () => {
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 2, pendingOpCount: 9, groupId: 8, reason: 'amended' })

    expect(reviewOf(7)?.changedUnderReview).toBe(false)
    expect(reviewOf(8)?.changedUnderReview).toBe(true)
  })

  it('stays quiet for a whole sweep landing, which names no group', async () => {
    await openSuggestedOps()

    emit?.({ pendingGroupCount: 3, pendingOpCount: 20, groupId: null, reason: 'proposed' })

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('stops listening once the dialog closes', async () => {
    await openSuggestedOps()
    closeSuggestedOps()

    expect(emit).toBeNull()
  })
})

describe('a group that changed under the review', () => {
  it('announces the change and leaves the rows exactly where they are', async () => {
    await openSuggestedOps()
    const before = reviewOf(BIG)?.window?.ops[0]?.opId

    // The agent amended the group: same id, a new revision.
    listMock.mockResolvedValueOnce([sweep([group(BIG, 9, { revision: 2 })])])
    await refreshSuggestions()

    expect(reviewOf(BIG)?.changedUnderReview).toBe(true)
    expect(reviewOf(BIG)?.window?.ops[0]?.opId).toBe(before)
  })

  it("stays quiet when only the counts moved, as the user's own refused approval moves them", async () => {
    await openSuggestedOps()

    // Same revision: an approval that excluded rows and then refused to start changes the live
    // count, not the group.
    listMock.mockResolvedValueOnce([sweep([group(BIG, 59_990, { totalOpCount: 60_000 })])])
    await refreshSuggestions()

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('stays quiet when nothing about the group moved', async () => {
    await openSuggestedOps()

    await refreshSuggestions()

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
  })

  it('drops the review when the group the user was reading is gone', async () => {
    await openSuggestedOps()

    listMock.mockResolvedValueOnce([])
    await refreshSuggestions()

    expect(reviewOf(BIG)).toBeUndefined()
  })

  it('reloads on request, and keeps what the user turned off', async () => {
    await openSuggestedOps()
    toggleOp(BIG, pending(2))
    emit?.({ pendingGroupCount: 1, pendingOpCount: 3, groupId: BIG, reason: 'amended' })
    pageMock.mockClear()

    await reloadGroup(BIG)

    expect(reviewOf(BIG)?.changedUnderReview).toBe(false)
    expect(pageMock).toHaveBeenCalledTimes(1)
    expect(isOpSelected(BIG, pending(2))).toBe(false)
  })
})

describe('the revision the user read', () => {
  it('sends the revision whose rows are on screen', async () => {
    listMock.mockResolvedValue([sweep([group(BIG, 60_000, { revision: 4 })])])
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(approveMock.mock.calls[0]?.[0]).toMatchObject({ groupId: BIG, revision: 4 })
  })

  it("won't approve a newer revision the user hasn't read, and sends it once they have", async () => {
    await openSuggestedOps()
    listMock.mockResolvedValue([sweep([group(BIG, 60_000, { revision: 2 })])])
    await refreshSuggestions()

    await approveGroup(BIG)
    expect(approveMock).not.toHaveBeenCalled()

    await reloadGroup(BIG)
    await approveGroup(BIG)
    expect(approveMock.mock.calls[0]?.[0]).toMatchObject({ groupId: BIG, revision: 2 })
  })
})

describe('rejecting', () => {
  it('records the no and re-reads', async () => {
    await openSuggestedOps()
    listMock.mockResolvedValueOnce([])

    await rejectGroup(BIG)

    expect(rejectMock).toHaveBeenCalledWith(BIG)
    expect(reviewOf(BIG)).toBeUndefined()
    expect(suggestedOpsState.sweeps).toEqual([])
    expect(suggestedOpsState.busyGroupId).toBeNull()
  })

  it('re-reads rather than insisting when somebody already answered the group', async () => {
    rejectMock.mockResolvedValueOnce({ kind: 'alreadyAnswered' })
    await openSuggestedOps()

    await rejectGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(2)
    expect(suggestedOpsState.loadError).toBe(false)
  })

  it('leaves the dialog usable when the rejection throws', async () => {
    rejectMock.mockRejectedValueOnce(new Error('gone'))
    await openSuggestedOps()

    await rejectGroup(BIG)

    expect(suggestedOpsState.busyGroupId).toBeNull()
    expect(suggestedOpsState.open).toBe(true)
  })
})

describe('a read or an answer that does not happen', () => {
  it('says the files could not load, and loads them when asked again', async () => {
    pageMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'store', detail: 'database is locked' }))

    await openSuggestedOps()

    expect(reviewOf(BIG)?.windowError).toBe(true)

    await ensureOpWindow(BIG, 0)

    expect(reviewOf(BIG)?.windowError).toBe(false)
    expect(opAt(BIG, 0)).not.toBeNull()
  })

  it('re-reads after an approval nobody can vouch for, and says to check the queue', async () => {
    // The worker thread died, so whether the group was claimed is unknown. The re-read shows
    // what the store holds; the notice keeps the person from approving twice on a guess.
    approveMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'approvalDidntFinish' }))
    await openSuggestedOps()
    listMock.mockClear()

    await approveGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(1)
    expect(suggestedOpsState.decisionNotice).toBe('suggestedOps.approvalUnsure')
    expect(suggestedOpsState.busyGroupId).toBeNull()
  })

  it('re-reads after an approval the store refused, and asks to try again', async () => {
    approveMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'store', detail: 'disk I/O error' }))
    await openSuggestedOps()
    listMock.mockClear()

    await approveGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(1)
    expect(suggestedOpsState.decisionNotice).toBe('suggestedOps.decisionNotRecorded')
  })

  it('re-reads after a rejection that did not record, and says so', async () => {
    rejectMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'storeNotOpen' }))
    await openSuggestedOps()
    listMock.mockClear()

    await rejectGroup(BIG)

    expect(listMock).toHaveBeenCalledTimes(1)
    expect(suggestedOpsState.decisionNotice).toBe('suggestedOps.decisionNotRecorded')
  })

  it('takes the notice down once the next answer lands', async () => {
    approveMock.mockRejectedValueOnce(new SuggestedOpsFailure({ type: 'store', detail: 'disk I/O error' }))
    listMock.mockResolvedValue([sweep([group(7, 3), group(8, 3)])])
    await openSuggestedOps()
    await approveGroup(7)

    await rejectGroup(8)

    expect(suggestedOpsState.decisionNotice).toBeNull()
  })
})

describe('closing', () => {
  it('reopens over a fresh review, with the last one’s deselection gone', async () => {
    await openSuggestedOps()
    toggleOp(BIG, pending(1))

    closeSuggestedOps()
    expect(suggestedOpsState.open).toBe(false)
    await openSuggestedOps()

    expect(isOpSelected(BIG, pending(1))).toBe(true)
  })
})

describe('a file that’s gone', () => {
  /** Live facts that say op 3 is gone and op 5's new name is taken, the rest present and free. */
  function factsSaying() {
    factsMock.mockImplementation(({ offset, limit }) =>
      Promise.resolve({
        kind: 'facts',
        facts: Array.from({ length: limit }, (_, i) => {
          const opId = offset + i
          return {
            opId,
            state: opId === 3 ? { kind: 'missing' } : { kind: 'present', size: 2048, modified: 1_790_000_000 },
            newName: opId === 5 ? 'taken' : null,
          }
        }),
      }),
    )
  }

  it('reads unticked, can’t be ticked back on, and counts for nothing', async () => {
    factsSaying()
    await openSuggestedOps()
    const gone = opAt(BIG, 3)
    if (!gone) throw new Error('row 3 is held')

    expect(isOpMissing(BIG, gone)).toBe(true)
    expect(isOpSelected(BIG, gone)).toBe(false)
    expect(approvableCount(BIG)).toBe(60_000 - 1)
    toggleOp(BIG, gone)
    expect(isOpSelected(BIG, gone)).toBe(false)
    expect(approvableCount(BIG)).toBe(60_000 - 1)
  })

  it('goes out turned off, as a flip over the stored ticks', async () => {
    factsSaying()
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith({
      groupId: BIG,
      revision: 1,
      selection: { mode: 'fromStored', excludeOpIds: [3], includeOpIds: [] },
    })
  })

  it('stays out after select all, and isn’t double-counted after select none', async () => {
    factsSaying()
    await openSuggestedOps()
    toggleAllOps(BIG) // all → none
    expect(approvableCount(BIG)).toBe(0)
    toggleAllOps(BIG) // none → all
    expect(approvableCount(BIG)).toBe(60_000 - 1)
    expect(selectionState(BIG)).toBe('all')

    await approveGroup(BIG)

    expect(approveMock).toHaveBeenCalledWith({
      groupId: BIG,
      revision: 1,
      selection: { mode: 'allExcept', opIds: [3] },
    })
  })

  it('marks a row whose new name is already taken where it lands (#774)', async () => {
    factsSaying()
    await openSuggestedOps()
    const taken = opAt(BIG, 5)
    const free = opAt(BIG, 4)
    if (!taken || !free) throw new Error('rows are held')

    expect(isNewNameTaken(BIG, taken)).toBe(true)
    expect(isNewNameTaken(BIG, free)).toBe(false)
  })
})

describe('a trash on a drive with no Trash', () => {
  it('approves with the fallback the review showed, so the backend deletes only then', async () => {
    listMock.mockResolvedValue([sweep([group(BIG, 3, { verb: 'trash', runsAs: 'permanentDelete' })])])
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(runsAsMock).toHaveBeenCalledWith('permanentDelete')
  })

  it('sends no fallback for a group that runs as itself', async () => {
    await openSuggestedOps()

    await approveGroup(BIG)

    expect(runsAsMock).toHaveBeenCalledWith(null)
  })
})
