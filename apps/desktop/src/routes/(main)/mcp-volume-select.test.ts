/**
 * MCP `select_volume`'s reply: it goes out only once the pane has come to rest, and it
 * says where. Acking at the optimistic commit let the switch's remembered-folder
 * correction land AFTER the reply, where it superseded the agent's next `nav_to_path`.
 *
 * The fake explorer moves the pane the way a real switch does: the commit puts it on
 * the volume's root and starts a listing, the correction then moves it to the folder
 * remembered there, and that folder's listing comes to rest. Fake timers drive the wait.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import type { ExplorerAPI } from './explorer-api'
import type { NavigateResult } from '$lib/file-explorer/pane/navigate'
import type { VolumeSelectOutcome } from '$lib/file-explorer/pane/volume-selection'
import { NAV_QUIET_WAIT } from './mcp-nav-landing'
import { selectVolumeForMcp } from './mcp-volume-select'

interface FakePane {
  volumeId: string
  path: string
  listingId: string | null
  loading: boolean
  stalled?: boolean
}

/**
 * A pane plus an explorer whose `selectVolumeByName` runs `onSelect` (the switch's
 * commit) and hands back `outcome`. `calls` records the order of the explorer calls
 * the reply depends on.
 */
function fakeExplorer(start: FakePane, onSelect: (pane: FakePane) => VolumeSelectOutcome) {
  const pane = { ...start }
  const calls: string[] = []
  const explorer = {
    selectVolumeByName: vi.fn((): Promise<VolumeSelectOutcome> => Promise.resolve(onSelect(pane))),
    selectVolumeById: vi.fn((): Promise<VolumeSelectOutcome> => Promise.resolve(onSelect(pane))),
    getPaneLocation: () => ({ volumeId: pane.volumeId, volumePath: '/', path: pane.path }),
    getPaneListingId: () => pane.listingId,
    isPaneLoading: () => pane.loading,
    isPaneStalled: () => pane.stalled ?? false,
    syncPaneStateToMcp: vi.fn(() => {
      calls.push('sync')
      return Promise.resolve()
    }),
  }
  vi.mocked(emit).mockImplementation(() => {
    calls.push('reply')
    return Promise.resolve()
  })
  return { pane, explorer: explorer as unknown as ExplorerAPI & typeof explorer, calls }
}

/** A switch whose background correction the test resolves when it wants to. */
function pendingCorrection(volumeId: string) {
  let resolve: () => void = () => {}
  const corrected = new Promise<void>((r) => {
    resolve = r
  })
  const navigation: NavigateResult = { status: 'started', settled: Promise.resolve(), corrected }
  return { outcome: { kind: 'selected', volumeId, navigation } as VolumeSelectOutcome, resolve }
}

describe('selectVolumeForMcp', () => {
  beforeEach(() => {
    vi.mocked(emit).mockReset()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('replies only once the correction has landed and the folder it chose has come to rest', async () => {
    const correction = pendingCorrection('mtp-1')
    const { pane, explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'mtp-1'
        p.path = 'mtp://1/65537'
        p.listingId = 'L1'
        p.loading = true
        return correction.outcome
      },
    )

    const done = selectVolumeForMcp({ explorer, pane: 'left', name: 'Internal Storage', requestId: 'req-1' })
    // The root's listing even comes to rest, but the correction hasn't decided yet.
    pane.loading = false
    await vi.advanceTimersByTimeAsync(2_000)
    expect(emit).not.toHaveBeenCalled()

    // The correction reopens the remembered folder, which starts its own listing.
    pane.path = 'mtp://1/65537/Documents'
    pane.listingId = 'L2'
    pane.loading = true
    correction.resolve()
    await vi.advanceTimersByTimeAsync(1_000)
    expect(emit).not.toHaveBeenCalled()

    pane.loading = false
    await vi.advanceTimersByTimeAsync(1_000)
    await done

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-1',
      ok: true,
      outcome: 'navigated',
      volumeId: 'mtp-1',
      path: 'mtp://1/65537/Documents',
    })
  })

  it('replies `stalled` as soon as the folder the switch opened stops answering', async () => {
    const correction = pendingCorrection('nas')
    const { pane, explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'nas'
        p.path = '/Volumes/nas/photos'
        p.listingId = 'L1'
        p.loading = true
        return correction.outcome
      },
    )
    correction.resolve()

    const done = selectVolumeForMcp({ explorer, pane: 'left', name: 'nas', requestId: 'req-stall' })
    await vi.advanceTimersByTimeAsync(300)
    expect(emit).not.toHaveBeenCalled()

    // A stalled listing keeps loading and retrying: it would never come to rest.
    pane.stalled = true
    await vi.advanceTimersByTimeAsync(200)
    await done

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-stall',
      ok: false,
      outcome: 'stalled',
      volumeId: 'nas',
      path: '/Volumes/nas/photos',
    })
  })

  it('selects by volume id when both id and display name are present', async () => {
    const correction = pendingCorrection('adb-pixel')
    const { explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'adb-pixel'
        p.path = 'adb://pixel'
        p.listingId = 'L1'
        return correction.outcome
      },
    )
    correction.resolve()

    const done = selectVolumeForMcp({
      explorer,
      pane: 'left',
      name: 'Pixel 9',
      volumeId: 'adb-pixel',
      requestId: 'req-id',
    })
    await vi.advanceTimersByTimeAsync(1_000)
    await done

    expect(explorer.selectVolumeById).toHaveBeenCalledExactlyOnceWith('left', 'adb-pixel')
    expect(explorer.selectVolumeByName).not.toHaveBeenCalled()
    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-id',
      ok: true,
      outcome: 'navigated',
      volumeId: 'adb-pixel',
      path: 'adb://pixel',
    })
  })

  it('flushes the pane state to the backend before replying, so `cmdr://state` shows the landing', async () => {
    const correction = pendingCorrection('mtp-1')
    const { pane, explorer, calls } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'mtp-1'
        p.path = 'mtp://1/65537'
        p.listingId = 'L1'
        return correction.outcome
      },
    )
    correction.resolve()

    const done = selectVolumeForMcp({ explorer, pane: 'left', name: 'Internal Storage', requestId: 'req-2' })
    await vi.advanceTimersByTimeAsync(1_000)
    await done

    expect(pane.path).toBe('mtp://1/65537')
    expect(calls).toEqual(['sync', 'reply'])
  })

  it('answers straight away when the pane was already where the select put it, with nothing to re-list', async () => {
    // Re-selecting the volume a pane shows, at the folder it shows: no prop changes, so no
    // listing ever starts. Waiting for one would time the tool out on a no-op.
    const correction = pendingCorrection('root')
    const { explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      () => correction.outcome,
    )
    correction.resolve()

    const done = selectVolumeForMcp({ explorer, pane: 'right', name: 'Macintosh HD', requestId: 'req-3' })
    await vi.advanceTimersByTimeAsync(1_000)
    await done

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-3',
      ok: true,
      outcome: 'navigated',
      volumeId: 'root',
      path: '/Users/david',
    })
  })

  it('answers without a listing on the servers hub, which never lists', async () => {
    const correction = pendingCorrection('network')
    const { explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'network'
        p.path = 'smb://'
        return correction.outcome
      },
    )
    correction.resolve()

    const done = selectVolumeForMcp({ explorer, pane: 'left', name: 'Servers', requestId: 'req-4' })
    await vi.advanceTimersByTimeAsync(1_000)
    await done

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-4',
      ok: true,
      outcome: 'navigated',
      volumeId: 'network',
      path: 'smb://',
    })
  })

  it('reports `fell-back` with the resting place when the volume fails to list and an edge flow moves the pane', async () => {
    const correction = pendingCorrection('mtp-1')
    const { pane, explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'mtp-1'
        p.path = 'mtp://1/65537'
        p.listingId = 'L1'
        p.loading = true
        return correction.outcome
      },
    )
    correction.resolve()

    const done = selectVolumeForMcp({ explorer, pane: 'left', name: 'Internal Storage', requestId: 'req-5' })
    await vi.advanceTimersByTimeAsync(300)
    // The listing dies and the MTP-fatal fallback takes the pane home.
    pane.volumeId = 'root'
    pane.path = '/Users/david'
    pane.listingId = 'L2'
    pane.loading = false
    await vi.advanceTimersByTimeAsync(1_000)
    await done

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-5',
      ok: false,
      outcome: 'fell-back',
      volumeId: 'root',
      path: '/Users/david',
    })
  })

  it('reports `did-not-settle` when the new volume never finishes listing', async () => {
    const correction = pendingCorrection('mtp-1')
    const { explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      (p) => {
        p.volumeId = 'mtp-1'
        p.path = 'mtp://1/65537'
        p.listingId = 'L1'
        p.loading = true
        return correction.outcome
      },
    )
    correction.resolve()

    const done = selectVolumeForMcp({ explorer, pane: 'left', name: 'Internal Storage', requestId: 'req-6' })
    await vi.advanceTimersByTimeAsync(NAV_QUIET_WAIT.budgetMs + 1_000)
    await done

    expect(emit).toHaveBeenCalledWith('mcp-response', {
      requestId: 'req-6',
      ok: false,
      outcome: 'did-not-settle',
      volumeId: 'mtp-1',
      path: 'mtp://1/65537',
    })
  })

  it('declines with a reply when no volume has that name', async () => {
    const { explorer } = fakeExplorer(
      { volumeId: 'root', path: '/Users/david', listingId: 'L0', loading: false },
      () => ({ kind: 'not-found' }),
    )

    await selectVolumeForMcp({ explorer, pane: 'left', name: 'Nope', requestId: 'req-7' })

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-7',
      ok: false,
      error: "Volume 'Nope' not found",
    })
  })

  it('declines with a reply when no explorer is mounted, instead of leaving the backend to time out', async () => {
    await selectVolumeForMcp({ explorer: undefined, pane: 'left', name: 'Macintosh HD', requestId: 'req-8' })

    expect(emit).toHaveBeenCalledExactlyOnceWith('mcp-response', {
      requestId: 'req-8',
      ok: false,
      error: 'Explorer is not ready',
    })
  })

  it('switches without waiting or replying for a fire-and-forget caller', async () => {
    // The E2E harness resets panes by raw event with no `requestId`: nobody to answer.
    const correction = pendingCorrection('root')
    const { explorer } = fakeExplorer(
      { volumeId: 'mtp-1', path: 'mtp://1/65537', listingId: 'L0', loading: false },
      () => correction.outcome,
    )

    await selectVolumeForMcp({ explorer, pane: 'left', name: 'Macintosh HD', requestId: undefined })

    expect(explorer.selectVolumeByName).toHaveBeenCalledExactlyOnceWith('left', 'Macintosh HD')
    expect(explorer.syncPaneStateToMcp).not.toHaveBeenCalled()
    expect(emit).not.toHaveBeenCalled()
  })
})
