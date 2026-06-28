import { describe, it, expect, vi, beforeEach } from 'vitest'

const { resolveLocationMock, addToastMock } = vi.hoisted(() => ({
  resolveLocationMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
}))
vi.mock('./resolve-location', () => ({ resolveLocation: resolveLocationMock }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastMock }))

import { navigateToDirInPane, navigateToFileInPane, revealSearchResultInPane } from './navigate-and-select'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'
import type { NavigateResult } from '../pane/navigate'
import type { Location } from '$lib/tauri-commands'

/** A `Location` (volume id + path) — the resolved input these helpers now take. */
function loc(path: string, volumeId = 'root'): Location {
  return { volumeId, path }
}

/** A `started` result whose `settled` is the given promise (defaults to resolved). */
function started(settled: Promise<void> = Promise.resolve()): NavigateResult {
  return { status: 'started', settled }
}

/** A `refused` result carrying the given exact message. */
function refused(message: string): NavigateResult {
  return { status: 'refused', reason: { kind: 'no-volume-resolved', message } }
}

function makeExplorer(navResult: NavigateResult) {
  const navigate = vi.fn(() => navResult)
  const moveCursor = vi.fn(() => Promise.resolve())
  const explorer = { navigate, moveCursor } as unknown as ExplorerAPI
  return { explorer, navigate, moveCursor }
}

describe('navigateToDirInPane', () => {
  it('navigates to the dir location and never moves the cursor', async () => {
    const { explorer, navigate, moveCursor } = makeExplorer(started())
    await navigateToDirInPane(explorer, 'left', loc('/tmp/here'))
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { location: loc('/tmp/here') }, source: 'user' })
    expect(moveCursor).not.toHaveBeenCalled()
  })

  it('bails on a refusal without throwing', async () => {
    const { explorer, moveCursor } = makeExplorer(refused('snapshot pane on a missing volume'))
    await expect(navigateToDirInPane(explorer, 'right', loc('/tmp'))).resolves.toBeUndefined()
    expect(moveCursor).not.toHaveBeenCalled()
  })
})

describe('navigateToFileInPane', () => {
  let moveCursorCalls: unknown[][]

  beforeEach(() => {
    moveCursorCalls = []
  })

  it('navigates to the parent location, then moves the cursor onto the file', async () => {
    const { explorer, navigate, moveCursor } = makeExplorer(started())
    await navigateToFileInPane(explorer, 'left', loc('/tmp'), 'a.txt')
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { location: loc('/tmp') }, source: 'user' })
    expect(moveCursor).toHaveBeenCalledWith('left', 'a.txt')
  })

  it('awaits the navigation settle before moving the cursor', async () => {
    let resolveListing!: () => void
    const listing = new Promise<void>((resolve) => {
      resolveListing = resolve
    })
    const navigate = vi.fn(() => started(listing))
    const moveCursor = vi.fn(() => {
      moveCursorCalls.push(['called'])
      return Promise.resolve()
    })
    const explorer = { navigate, moveCursor } as unknown as ExplorerAPI

    const promise = navigateToFileInPane(explorer, 'left', loc('/tmp'), 'a.txt')
    // Cursor must NOT move before the navigation settles.
    expect(moveCursorCalls).toHaveLength(0)
    resolveListing()
    await promise
    expect(moveCursorCalls).toHaveLength(1)
  })

  it('bails on a refusal and never moves the cursor', async () => {
    const { explorer, moveCursor } = makeExplorer(refused('cannot start'))
    await navigateToFileInPane(explorer, 'left', loc('/tmp'), 'a.txt')
    expect(moveCursor).not.toHaveBeenCalled()
  })
})

describe('revealSearchResultInPane (the search "Go to file" edge)', () => {
  beforeEach(() => {
    resolveLocationMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
  })

  it("resolves the result's PARENT dir, navigates with that location, then moves the cursor onto the file", async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: loc('/Volumes/Nas/docs', 'nas') })
    const { explorer, navigate, moveCursor } = makeExplorer(started())

    await revealSearchResultInPane(explorer, 'left', '/Volumes/Nas/docs/report.pdf')

    // It resolves the parent dir, not the file path.
    expect(resolveLocationMock).toHaveBeenCalledWith('/Volumes/Nas/docs')
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { location: loc('/Volumes/Nas/docs', 'nas') },
      source: 'user',
    })
    expect(moveCursor).toHaveBeenCalledWith('left', 'report.pdf')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('shows the friendly toast and does NOT navigate when the parent volume is unresolvable', async () => {
    resolveLocationMock.mockResolvedValue({ ok: false, reason: 'no-volume' })
    const { explorer, navigate, moveCursor } = makeExplorer(started())

    await revealSearchResultInPane(explorer, 'left', '/Volumes/Gone/docs/report.pdf')

    expect(navigate).not.toHaveBeenCalled()
    expect(moveCursor).not.toHaveBeenCalled()
    expect(addToastMock).toHaveBeenCalledWith("Couldn't reach that location's drive. It might be disconnected.", {
      level: 'info',
    })
  })
})
