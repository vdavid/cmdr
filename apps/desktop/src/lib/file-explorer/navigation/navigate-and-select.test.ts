import { describe, it, expect, vi, beforeEach } from 'vitest'

const { resolveLocationMock, addToastMock } = vi.hoisted(() => ({
  resolveLocationMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
}))
vi.mock('./resolve-location', () => ({ resolveLocation: resolveLocationMock }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: addToastMock }))

import {
  navigateToDirInPane,
  navigateToFileInPane,
  revealPathInBestPane,
  revealSearchResultInPane,
} from './navigate-and-select'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'
import type { NavigateResult } from '../pane/navigate'
import type { Location } from '#lib/tauri-commands/index.js'

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
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { goTo: loc('/tmp/here') }, source: 'user' })
    expect(moveCursor).not.toHaveBeenCalled()
  })

  it('reports success so a caller can trust the pane got there', async () => {
    const { explorer } = makeExplorer(started())
    await expect(navigateToDirInPane(explorer, 'left', loc('/tmp/here'))).resolves.toBe(true)
  })

  it('bails on a refusal without throwing, and reports it', async () => {
    const { explorer, moveCursor } = makeExplorer(refused('snapshot pane on a missing volume'))
    // `false`, not `undefined`: the pane still holds its previous directory's
    // valid listing id, so a caller reading pane state can't detect the refusal.
    await expect(navigateToDirInPane(explorer, 'right', loc('/tmp'))).resolves.toBe(false)
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
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { goTo: loc('/tmp') }, source: 'user' })
    expect(moveCursor).toHaveBeenCalledWith('left', 'a.txt', 'person')
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

  it('crosses from the snapshot pane to the result’s resolved volume, then lands on the file', async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: loc('/Volumes/Nas/docs', 'nas') })
    const { explorer, navigate, moveCursor } = makeExplorer(started())

    await revealSearchResultInPane(explorer, 'left', '/Volumes/Nas/docs/report.pdf')

    // The display-only `parentPath` could say `~/docs`; the full result path is the
    // source of truth. Resolve its parent, and preserve the resolver's cross-volume id.
    expect(resolveLocationMock).toHaveBeenCalledWith('/Volumes/Nas/docs')
    expect(navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { goTo: loc('/Volumes/Nas/docs', 'nas') },
      source: 'user',
    })
    expect(moveCursor).toHaveBeenCalledWith('left', 'report.pdf', 'person')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('shows the friendly toast and does NOT navigate when the parent volume is unresolvable', async () => {
    resolveLocationMock.mockResolvedValue({ ok: false, reason: 'no-volume' })
    const { explorer, navigate, moveCursor } = makeExplorer(started())

    await revealSearchResultInPane(explorer, 'left', '/Volumes/Gone/docs/report.pdf')

    expect(navigate).not.toHaveBeenCalled()
    expect(moveCursor).not.toHaveBeenCalled()
    expect(addToastMock).toHaveBeenCalledWith('Couldn’t reach that location’s drive. It might be disconnected.', {
      level: 'info',
    })
  })
})

describe('revealPathInBestPane (the Ask Cmdr tool line’s path links)', () => {
  beforeEach(() => {
    resolveLocationMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
  })

  /** An explorer whose focused left pane shows `/elsewhere`, so every reveal navigates. */
  function bestPaneExplorer(moveCursor = vi.fn(() => Promise.resolve())) {
    const navigate = vi.fn(() => started())
    const explorer = {
      navigate,
      moveCursor,
      getFocusedPane: () => 'left' as const,
      setFocusedPane: vi.fn(),
      getPaneLocation: () => ({ volumeId: 'root', volumePath: '/', path: '/elsewhere' }),
    } as unknown as ExplorerAPI
    return { explorer, navigate, moveCursor }
  }

  it('opens a folder in the focused pane', async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: loc('/Users/me/Downloads') })
    const { explorer, navigate, moveCursor } = bestPaneExplorer()

    await revealPathInBestPane(explorer, { path: '/Users/me/Downloads', kind: 'folder' })

    expect(resolveLocationMock).toHaveBeenCalledWith('/Users/me/Downloads')
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { goTo: loc('/Users/me/Downloads') }, source: 'user' })
    expect(moveCursor).not.toHaveBeenCalled()
  })

  it('opens a file’s folder and lands on the file', async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: loc('/Users/me') })
    const { explorer, moveCursor } = bestPaneExplorer()

    await revealPathInBestPane(explorer, { path: '/Users/me/notes.txt', kind: 'file' })

    expect(resolveLocationMock).toHaveBeenCalledWith('/Users/me')
    expect(moveCursor).toHaveBeenCalledWith('left', 'notes.txt', 'person')
  })

  it('stays in the folder when the file isn’t in the listing (a hidden dotfile), never throwing', async () => {
    resolveLocationMock.mockResolvedValue({ ok: true, location: loc('/Users/me') })
    const { explorer, navigate } = bestPaneExplorer(vi.fn(() => Promise.reject(new Error('no such entry'))))

    await expect(revealPathInBestPane(explorer, { path: '/Users/me/.env', kind: 'file' })).resolves.toBeUndefined()
    expect(navigate).toHaveBeenCalled()
  })

  it('shows the friendly toast and goes nowhere when the drive can’t be reached', async () => {
    resolveLocationMock.mockResolvedValue({ ok: false, reason: 'no-volume' })
    const { explorer, navigate } = bestPaneExplorer()

    await revealPathInBestPane(explorer, { path: '/Volumes/Gone/x', kind: 'folder' })

    expect(navigate).not.toHaveBeenCalled()
    expect(addToastMock).toHaveBeenCalled()
  })
})
