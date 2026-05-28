import { describe, it, expect, vi, beforeEach } from 'vitest'

// Hoisted mocks: vi.mock factories cannot reference top-level bindings, so we
// declare the spy fns inside vi.hoisted() and reuse them from the mock factories
// AND from the test body.
const {
  revealLatestDownloadMock,
  downloadsWatcherStatusMock,
  addToastMock,
  openPrivacySettingsMock,
  navigateToPathMock,
  moveCursorMock,
  getFocusedPaneMock,
} = vi.hoisted(() => ({
  revealLatestDownloadMock: vi.fn(),
  downloadsWatcherStatusMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
  openPrivacySettingsMock: vi.fn(() => Promise.resolve()),
  navigateToPathMock: vi.fn(() => Promise.resolve()),
  moveCursorMock: vi.fn(() => Promise.resolve()),
  getFocusedPaneMock: vi.fn(() => 'left' as 'left' | 'right'),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    revealLatestDownload: revealLatestDownloadMock,
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
vi.mock('./RevealEmptyToastContent.svelte', () => ({
  default: { __toastContent: 'RevealEmptyToastContent' },
  setEmptyToastHandler: vi.fn(),
}))
vi.mock('./RevealFdaToastContent.svelte', () => ({
  default: { __toastContent: 'RevealFdaToastContent' },
}))

import { revealLatestDownload, REVEAL_EMPTY_TOAST_ID, REVEAL_FDA_TOAST_ID } from './reveal'
import RevealEmptyToastContent from './RevealEmptyToastContent.svelte'
import RevealFdaToastContent from './RevealFdaToastContent.svelte'
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

describe('revealLatestDownload', () => {
  beforeEach(() => {
    revealLatestDownloadMock.mockReset()
    downloadsWatcherStatusMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
    openPrivacySettingsMock.mockReset().mockResolvedValue(undefined)
    navigateToPathMock.mockReset().mockResolvedValue(undefined)
    moveCursorMock.mockReset().mockResolvedValue(undefined)
    getFocusedPaneMock.mockReset().mockReturnValue('left')
  })

  it('navigates the focused pane and selects the file on success', async () => {
    revealLatestDownloadMock.mockResolvedValue({
      status: 'ok',
      data: {
        path: '/Users/me/Downloads/report.pdf',
        parentDir: '/Users/me/Downloads',
        fileName: 'report.pdf',
      },
    })
    getFocusedPaneMock.mockReturnValue('right')

    await revealLatestDownload(makeExplorerStub())

    expect(navigateToPathMock).toHaveBeenCalledWith('right', '/Users/me/Downloads')
    expect(moveCursorMock).toHaveBeenCalledWith('right', 'report.pdf')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('shows the empty INFO toast with the dedup id on RevealError::Empty', async () => {
    revealLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'empty' },
    })
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false, lastDetected: null },
    })

    await revealLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    expect(content).toBe(RevealEmptyToastContent)
    expect(options).toMatchObject({
      id: REVEAL_EMPTY_TOAST_ID,
      level: 'info',
    })
    expect(navigateToPathMock).not.toHaveBeenCalled()
    expect(moveCursorMock).not.toHaveBeenCalled()
  })

  it('shows the FDA INFO toast with the dedup id on RevealError::WatcherUnavailable', async () => {
    revealLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'watcherUnavailable' },
    })

    await revealLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    expect(content).toBe(RevealFdaToastContent)
    expect(options).toMatchObject({
      id: REVEAL_FDA_TOAST_ID,
      level: 'info',
    })
  })

  it('shows the FDA INFO toast on RevealError::DownloadsDirUnresolved', async () => {
    // No `HOME`, no `dirs::download_dir`: nothing to navigate to. The user-facing
    // story is the same as the FDA case (we can't act on Downloads), so reuse the
    // toast instead of inventing a third state.
    revealLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'downloadsDirUnresolved' },
    })

    await revealLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    expect(content).toBe(RevealFdaToastContent)
  })

  it('dedups: two empty triggers in a row pass the same id so the toast replaces in place', async () => {
    revealLatestDownloadMock.mockResolvedValue({
      status: 'error',
      error: { kind: 'empty' },
    })
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false, lastDetected: null },
    })

    await revealLatestDownload(makeExplorerStub())
    await revealLatestDownload(makeExplorerStub())

    expect(addToastMock).toHaveBeenCalledTimes(2)
    // The toast store dedups by `id`: passing the same id makes the second
    // call replace in place rather than stack a second toast.
    const [, firstOptions] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    const [, secondOptions] = addToastMock.mock.calls[1] as unknown as [unknown, Record<string, unknown>]
    const firstId = firstOptions.id
    const secondId = secondOptions.id
    expect(firstId).toBe(REVEAL_EMPTY_TOAST_ID)
    expect(secondId).toBe(REVEAL_EMPTY_TOAST_ID)
  })

  it('does nothing when the explorer handle is missing (HMR / pre-mount)', async () => {
    await revealLatestDownload(undefined)

    expect(revealLatestDownloadMock).not.toHaveBeenCalled()
    expect(addToastMock).not.toHaveBeenCalled()
    expect(navigateToPathMock).not.toHaveBeenCalled()
  })
})
