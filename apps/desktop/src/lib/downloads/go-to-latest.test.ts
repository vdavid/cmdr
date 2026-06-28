import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { NavigateResult } from '$lib/file-explorer/pane/navigate'

// Hoisted mocks: vi.mock factories cannot reference top-level bindings, so we
// declare the spy fns inside vi.hoisted() and reuse them from the mock factories
// AND from the test body.
const {
  goToLatestDownloadMock,
  downloadsWatcherStatusMock,
  addToastMock,
  openPrivacySettingsMock,
  resolveLocationMock,
  navigateMock,
  moveCursorMock,
  getFocusedPaneMock,
  getPaneLocationMock,
  setFocusedPaneMock,
} = vi.hoisted(() => ({
  goToLatestDownloadMock: vi.fn(),
  downloadsWatcherStatusMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
  openPrivacySettingsMock: vi.fn(() => Promise.resolve()),
  // The download's parent dir resolves to a `Location` on the `root` volume.
  resolveLocationMock: vi.fn((path: string) => Promise.resolve({ ok: true as const, location: { volumeId: 'root', path } })),
  // `navigate()` returns a `NavigateResult`; default to a started no-op.
  navigateMock: vi.fn((): NavigateResult => ({ status: 'started', settled: Promise.resolve() })),
  moveCursorMock: vi.fn(() => Promise.resolve()),
  getFocusedPaneMock: vi.fn(() => 'left'),
  // Pane active-tab location getter. Default: both panes elsewhere, on the main
  // local volume, so the pane-reuse check fails and the focused pane navigates.
  // Typed with the `pane` param (matching the real `ExplorerAPI.getPaneLocation`)
  // so the per-pane `mockImplementation((p) => ...)` overrides below typecheck;
  // the default impl ignores it (both panes elsewhere).
  getPaneLocationMock: vi.fn<(pane: 'left' | 'right') => { volumeId: string; volumePath: string; path: string }>(
    () => ({ volumeId: 'root', volumePath: '/', path: '/Users/me/elsewhere' }),
  ),
  setFocusedPaneMock: vi.fn(),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    goToLatestDownload: goToLatestDownloadMock,
    downloadsWatcherStatus: downloadsWatcherStatusMock,
  },
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
}))

vi.mock('$lib/tauri-commands', () => ({
  openPrivacySettings: openPrivacySettingsMock,
}))

vi.mock('$lib/file-explorer/navigation/resolve-location', () => ({
  resolveLocation: resolveLocationMock,
}))

// Component imports return opaque module refs; we only assert that the same
// reference is passed to `addToast` (proves the dedup id wires the right toast).
vi.mock('./LatestDownloadEmptyToastContent.svelte', () => ({
  default: { __toastContent: 'LatestDownloadEmptyToastContent' },
}))
vi.mock('./LatestDownloadFdaToastContent.svelte', () => ({
  default: { __toastContent: 'LatestDownloadFdaToastContent' },
}))

import {
  goToLatestDownload,
  goToDownload,
  LATEST_DOWNLOAD_EMPTY_TOAST_ID,
  LATEST_DOWNLOAD_FDA_TOAST_ID,
} from './go-to-latest'
import LatestDownloadEmptyToastContent from './LatestDownloadEmptyToastContent.svelte'
import LatestDownloadFdaToastContent from './LatestDownloadFdaToastContent.svelte'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

/** Flush the queued microtasks (the async `onGoToDownloads` resolve-then-navigate). */
async function flushMicrotasks(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

function makeExplorerStub(): ExplorerAPI {
  // Only the methods these helpers touch need real stubs. Everything else is
  // unused; the typed stub avoids a `Partial<ExplorerAPI>` cast leaking into the
  // helper's call sites.
  return {
    getFocusedPane: getFocusedPaneMock,
    navigate: navigateMock,
    moveCursor: moveCursorMock,
    getPaneLocation: getPaneLocationMock,
    setFocusedPane: setFocusedPaneMock,
  } as unknown as ExplorerAPI
}

/**
 * Helper: wire `getPaneLocation` so a given pane's active tab shows `path` on a
 * real local volume (so the pane-reuse match succeeds), while the other pane
 * sits elsewhere. `volumeId`/`volumePath` default to the main local volume.
 */
function paneShows(
  pane: 'left' | 'right',
  path: string,
  volume: { volumeId: string; volumePath: string } = { volumeId: 'root', volumePath: '/' },
): void {
  getPaneLocationMock.mockImplementation((p: 'left' | 'right') =>
    p === pane ? { ...volume, path } : { volumeId: 'root', volumePath: '/', path: '/Users/me/elsewhere' },
  )
}

describe('goToLatestDownload', () => {
  beforeEach(() => {
    goToLatestDownloadMock.mockReset()
    downloadsWatcherStatusMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
    openPrivacySettingsMock.mockReset().mockResolvedValue(undefined)
    resolveLocationMock
      .mockReset()
      .mockImplementation((path: string) => Promise.resolve({ ok: true as const, location: { volumeId: 'root', path } }))
    navigateMock.mockReset().mockReturnValue({ status: 'started', settled: Promise.resolve() })
    moveCursorMock.mockReset().mockResolvedValue(undefined)
    getFocusedPaneMock.mockReset().mockReturnValue('left')
    getPaneLocationMock.mockReset().mockReturnValue({ volumeId: 'root', volumePath: '/', path: '/Users/me/elsewhere' })
    setFocusedPaneMock.mockReset()
  })

  it('navigates the focused pane and selects the file on success', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: {
        path: '/Users/me/Downloads/report.pdf',
        parentDir: '/Users/me/Downloads',
        fileName: 'report.pdf',
      },
    })
    getFocusedPaneMock.mockReturnValue('right')

    await goToLatestDownload(makeExplorerStub())

    expect(navigateMock).toHaveBeenCalledWith({ pane: 'right', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(moveCursorMock).toHaveBeenCalledWith('right', 'report.pdf')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('moves the cursor only (no navigate, no focus shift) when the focused pane already shows the dir', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: { path: '/Users/me/Downloads/report.pdf', parentDir: '/Users/me/Downloads', fileName: 'report.pdf' },
    })
    getFocusedPaneMock.mockReturnValue('left')
    paneShows('left', '/Users/me/Downloads')

    await goToLatestDownload(makeExplorerStub())

    expect(navigateMock).not.toHaveBeenCalled()
    expect(setFocusedPaneMock).not.toHaveBeenCalled()
    expect(moveCursorMock).toHaveBeenCalledWith('left', 'report.pdf')
  })

  it('shifts focus and moves the cursor (no navigate) when the OTHER pane already shows the dir', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: { path: '/Users/me/Downloads/report.pdf', parentDir: '/Users/me/Downloads', fileName: 'report.pdf' },
    })
    getFocusedPaneMock.mockReturnValue('left')
    paneShows('right', '/Users/me/Downloads')

    await goToLatestDownload(makeExplorerStub())

    expect(navigateMock).not.toHaveBeenCalled()
    expect(setFocusedPaneMock).toHaveBeenCalledWith('right')
    expect(moveCursorMock).toHaveBeenCalledWith('right', 'report.pdf')
  })

  it('navigates the focused pane when neither pane shows the dir', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: { path: '/Users/me/Downloads/report.pdf', parentDir: '/Users/me/Downloads', fileName: 'report.pdf' },
    })
    getFocusedPaneMock.mockReturnValue('left')
    // Both panes sit elsewhere (the beforeEach default).

    await goToLatestDownload(makeExplorerStub())

    expect(navigateMock).toHaveBeenCalledWith({ pane: 'left', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(setFocusedPaneMock).not.toHaveBeenCalled()
    expect(moveCursorMock).toHaveBeenCalledWith('left', 'report.pdf')
  })

  it('does NOT count a pane at an equal-looking path on a virtual or device volume as showing the dir', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: { path: '/Users/me/Downloads/report.pdf', parentDir: '/Users/me/Downloads', fileName: 'report.pdf' },
    })
    getFocusedPaneMock.mockReturnValue('left')
    // The other pane's active tab reports the exact same path string, but it's on
    // a network volume (its volumePath is `smb://…`, not a real local mount). It
    // must not count, so the focused pane navigates as usual.
    getPaneLocationMock.mockImplementation((p: 'left' | 'right') =>
      p === 'right'
        ? { volumeId: 'network', volumePath: 'smb://', path: '/Users/me/Downloads' }
        : { volumeId: 'root', volumePath: '/', path: '/Users/me/elsewhere' },
    )

    await goToLatestDownload(makeExplorerStub())

    expect(navigateMock).toHaveBeenCalledWith({ pane: 'left', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(setFocusedPaneMock).not.toHaveBeenCalled()
    expect(moveCursorMock).toHaveBeenCalledWith('left', 'report.pdf')
  })

  it('does NOT count an MTP pane whose path string matches the local dir', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: { path: '/Users/me/Downloads/report.pdf', parentDir: '/Users/me/Downloads', fileName: 'report.pdf' },
    })
    getFocusedPaneMock.mockReturnValue('left')
    // MTP device volume: volumePath is an `mtp://…` URL, so the local Downloads
    // path is not on it — `isPathOnVolume` rejects the match.
    getPaneLocationMock.mockImplementation((p: 'left' | 'right') =>
      p === 'right'
        ? { volumeId: 'mtp-0-1', volumePath: 'mtp://mtp-0-1/65537', path: '/Users/me/Downloads' }
        : { volumeId: 'root', volumePath: '/', path: '/Users/me/elsewhere' },
    )

    await goToLatestDownload(makeExplorerStub())

    expect(navigateMock).toHaveBeenCalledWith({ pane: 'left', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(setFocusedPaneMock).not.toHaveBeenCalled()
  })

  it('shows the empty INFO toast with the dedup id on GoToLatestError::Empty', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'empty' },
    })
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false },
    })

    await goToLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as unknown as [
      unknown,
      Record<string, unknown> & { props?: { onGoToDownloads: () => void } },
    ]
    expect(content).toBe(LatestDownloadEmptyToastContent)
    expect(options).toMatchObject({
      id: LATEST_DOWNLOAD_EMPTY_TOAST_ID,
      level: 'info',
    })
    // The "Go to Downloads" handler arrives as a prop (snapshotted closure
    // over the focused pane + Downloads dir), not via a module-state shim.
    expect(typeof options.props?.onGoToDownloads).toBe('function')
    // Invoking the prop triggers the snapshotted navigation (which resolves the
    // dir's volume first, so flush the microtasks before asserting).
    options.props?.onGoToDownloads()
    await flushMicrotasks()
    expect(navigateMock).toHaveBeenCalledWith({ pane: 'left', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(moveCursorMock).not.toHaveBeenCalled()
  })

  it('empty-toast "Go to Downloads" reuses a pane already showing Downloads, evaluated at click time', async () => {
    goToLatestDownloadMock.mockResolvedValue({ status: 'error', error: { kind: 'empty' } })
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false },
    })
    getFocusedPaneMock.mockReturnValue('left')

    await goToLatestDownload(makeExplorerStub())

    const [, options] = addToastMock.mock.calls[0] as unknown as [unknown, { props?: { onGoToDownloads: () => void } }]
    // The other pane navigates to Downloads AFTER the toast was added, so the
    // action must re-evaluate which pane shows the dir at CLICK time.
    paneShows('right', '/Users/me/Downloads')
    options.props?.onGoToDownloads()
    await flushMicrotasks()

    // Pane reuse: focus the pane that shows Downloads, no fresh navigation.
    expect(navigateMock).not.toHaveBeenCalled()
    expect(setFocusedPaneMock).toHaveBeenCalledWith('right')
  })

  it('shows the FDA INFO toast with the dedup id on GoToLatestError::WatcherUnavailable', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'watcherUnavailable' },
    })

    await goToLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    expect(content).toBe(LatestDownloadFdaToastContent)
    expect(options).toMatchObject({
      id: LATEST_DOWNLOAD_FDA_TOAST_ID,
      level: 'info',
    })
  })

  it('shows the FDA INFO toast on GoToLatestError::DownloadsDirUnresolved', async () => {
    // No `HOME`, no `dirs::download_dir`: nothing to navigate to. The user-facing
    // story is the same as the FDA case (we can't act on Downloads), so reuse the
    // toast instead of inventing a third state.
    goToLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'downloadsDirUnresolved' },
    })

    await goToLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    expect(content).toBe(LatestDownloadFdaToastContent)
  })

  it('dedups: two empty triggers in a row pass the same id so the toast replaces in place', async () => {
    goToLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'empty' },
    })
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false },
    })

    await goToLatestDownload(makeExplorerStub())
    await goToLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(2)
    // The toast store dedups by `id`: passing the same id makes the second
    // call replace in place rather than stack a second toast.
    const [, firstOptions] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    const [, secondOptions] = addToastMock.mock.calls[1] as unknown as [unknown, Record<string, unknown>]
    const firstId = firstOptions.id
    const secondId = secondOptions.id
    expect(firstId).toBe(LATEST_DOWNLOAD_EMPTY_TOAST_ID)
    expect(secondId).toBe(LATEST_DOWNLOAD_EMPTY_TOAST_ID)
  })

  it('does nothing when the explorer handle is missing (HMR / pre-mount)', async () => {
    await goToLatestDownload(undefined)

    expect(goToLatestDownloadMock).not.toHaveBeenCalled()
    expect(addToastMock).not.toHaveBeenCalled()
    expect(navigateMock).not.toHaveBeenCalled()
  })
})

describe('goToDownload', () => {
  beforeEach(() => {
    addToastMock.mockReset().mockReturnValue('toast-id')
    resolveLocationMock
      .mockReset()
      .mockImplementation((path: string) => Promise.resolve({ ok: true as const, location: { volumeId: 'root', path } }))
    navigateMock.mockReset().mockReturnValue({ status: 'started', settled: Promise.resolve() })
    moveCursorMock.mockReset().mockResolvedValue(undefined)
    getFocusedPaneMock.mockReset().mockReturnValue('left')
    getPaneLocationMock.mockReset().mockReturnValue({ volumeId: 'root', volumePath: '/', path: '/Users/me/elsewhere' })
    setFocusedPaneMock.mockReset()
  })

  it('navigates the focused pane to parentDir and selects the file when neither pane shows it', async () => {
    await goToDownload(makeExplorerStub(), '/Users/me/Downloads', 'report.pdf')

    expect(navigateMock).toHaveBeenCalledWith({ pane: 'left', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(moveCursorMock).toHaveBeenCalledWith('left', 'report.pdf')
  })

  it('reuses the focused pane (cursor only) when it already shows the dir', async () => {
    getFocusedPaneMock.mockReturnValue('left')
    paneShows('left', '/Users/me/Downloads')

    await goToDownload(makeExplorerStub(), '/Users/me/Downloads', 'report.pdf')

    expect(navigateMock).not.toHaveBeenCalled()
    expect(setFocusedPaneMock).not.toHaveBeenCalled()
    expect(moveCursorMock).toHaveBeenCalledWith('left', 'report.pdf')
  })

  it('does nothing when the explorer handle is missing (HMR / pre-mount)', async () => {
    await goToDownload(undefined, '/Users/me/Downloads', 'report.pdf')

    expect(navigateMock).not.toHaveBeenCalled()
    expect(moveCursorMock).not.toHaveBeenCalled()
  })

  it('bails out without moving the cursor when navigate refuses synchronously', async () => {
    navigateMock.mockReturnValueOnce({
      status: 'refused',
      reason: { kind: 'no-volume-resolved', message: 'snapshot pane on a missing volume' },
    })

    await goToDownload(makeExplorerStub(), '/Users/me/Downloads', 'report.pdf')

    expect(navigateMock).toHaveBeenCalledWith({ pane: 'left', to: { location: { volumeId: 'root', path: '/Users/me/Downloads' } }, source: 'user' })
    expect(moveCursorMock).not.toHaveBeenCalled()
  })
})
