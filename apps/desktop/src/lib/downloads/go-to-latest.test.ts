import { describe, it, expect, vi, beforeEach } from 'vitest'

// Hoisted mocks: vi.mock factories cannot reference top-level bindings, so we
// declare the spy fns inside vi.hoisted() and reuse them from the mock factories
// AND from the test body.
const {
  goToLatestDownloadMock,
  downloadsWatcherStatusMock,
  addToastMock,
  openPrivacySettingsMock,
  navigateToPathMock,
  moveCursorMock,
  getFocusedPaneMock,
} = vi.hoisted(() => ({
  goToLatestDownloadMock: vi.fn(),
  downloadsWatcherStatusMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
  openPrivacySettingsMock: vi.fn(() => Promise.resolve()),
  navigateToPathMock: vi.fn(() => Promise.resolve()),
  moveCursorMock: vi.fn(() => Promise.resolve()),
  getFocusedPaneMock: vi.fn(() => 'left'),
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

function makeExplorerStub(): ExplorerAPI {
  // Only the three methods this helper touches need real stubs. Everything
  // else is unused; the typed stub avoids a `Partial<ExplorerAPI>` cast leaking
  // into the helper's call sites.
  return {
    getFocusedPane: getFocusedPaneMock,
    navigateToPath: navigateToPathMock,
    moveCursor: moveCursorMock,
  } as unknown as ExplorerAPI
}

describe('goToLatestDownload', () => {
  beforeEach(() => {
    goToLatestDownloadMock.mockReset()
    downloadsWatcherStatusMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
    openPrivacySettingsMock.mockReset().mockResolvedValue(undefined)
    navigateToPathMock.mockReset().mockResolvedValue(undefined)
    moveCursorMock.mockReset().mockResolvedValue(undefined)
    getFocusedPaneMock.mockReset().mockReturnValue('left')
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

    expect(navigateToPathMock).toHaveBeenCalledWith('right', '/Users/me/Downloads')
    expect(moveCursorMock).toHaveBeenCalledWith('right', 'report.pdf')
    expect(addToastMock).not.toHaveBeenCalled()
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
    // Invoking the prop triggers the snapshotted navigation.
    options.props?.onGoToDownloads()
    expect(navigateToPathMock).toHaveBeenCalledWith('left', '/Users/me/Downloads')
    expect(moveCursorMock).not.toHaveBeenCalled()
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
    expect(navigateToPathMock).not.toHaveBeenCalled()
  })
})

describe('goToDownload', () => {
  beforeEach(() => {
    addToastMock.mockReset().mockReturnValue('toast-id')
    navigateToPathMock.mockReset().mockResolvedValue(undefined)
    moveCursorMock.mockReset().mockResolvedValue(undefined)
    getFocusedPaneMock.mockReset().mockReturnValue('left')
  })

  it('navigates the focused pane to parentDir and selects the file', async () => {
    await goToDownload(makeExplorerStub(), '/Users/me/Downloads', 'report.pdf')

    expect(navigateToPathMock).toHaveBeenCalledWith('left', '/Users/me/Downloads')
    expect(moveCursorMock).toHaveBeenCalledWith('left', 'report.pdf')
  })

  it('does nothing when the explorer handle is missing (HMR / pre-mount)', async () => {
    await goToDownload(undefined, '/Users/me/Downloads', 'report.pdf')

    expect(navigateToPathMock).not.toHaveBeenCalled()
    expect(moveCursorMock).not.toHaveBeenCalled()
  })

  it('bails out without moving the cursor when navigateToPath refuses synchronously', async () => {
    // `navigateToPath` returns `string | Promise<void>`; the mock factory's
    // inferred type pins it to `Promise<void>`, so cast for this one return path.
    ;(navigateToPathMock as unknown as { mockReturnValueOnce: (v: unknown) => void }).mockReturnValueOnce(
      'snapshot pane on a missing volume',
    )

    await goToDownload(makeExplorerStub(), '/Users/me/Downloads', 'report.pdf')

    expect(navigateToPathMock).toHaveBeenCalledWith('left', '/Users/me/Downloads')
    expect(moveCursorMock).not.toHaveBeenCalled()
  })
})
