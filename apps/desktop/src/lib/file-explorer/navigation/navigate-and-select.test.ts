import { describe, it, expect, vi, beforeEach } from 'vitest'
import { navigateToDirInPane, navigateToFileInPane } from './navigate-and-select'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'
import type { NavigateResult } from '../pane/navigate'

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
  it('navigates to the dir and never moves the cursor', async () => {
    const { explorer, navigate, moveCursor } = makeExplorer(started())
    await navigateToDirInPane(explorer, 'left', '/tmp/here')
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { path: '/tmp/here' }, source: 'user' })
    expect(moveCursor).not.toHaveBeenCalled()
  })

  it('bails on a refusal without throwing', async () => {
    const { explorer, moveCursor } = makeExplorer(refused('snapshot pane on a missing volume'))
    await expect(navigateToDirInPane(explorer, 'right', '/tmp')).resolves.toBeUndefined()
    expect(moveCursor).not.toHaveBeenCalled()
  })
})

describe('navigateToFileInPane', () => {
  let moveCursorCalls: unknown[][]

  beforeEach(() => {
    moveCursorCalls = []
  })

  it('navigates to the parent, then moves the cursor onto the file', async () => {
    const { explorer, navigate, moveCursor } = makeExplorer(started())
    await navigateToFileInPane(explorer, 'left', '/tmp', 'a.txt')
    expect(navigate).toHaveBeenCalledWith({ pane: 'left', to: { path: '/tmp' }, source: 'user' })
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

    const promise = navigateToFileInPane(explorer, 'left', '/tmp', 'a.txt')
    // Cursor must NOT move before the navigation settles.
    expect(moveCursorCalls).toHaveLength(0)
    resolveListing()
    await promise
    expect(moveCursorCalls).toHaveLength(1)
  })

  it('bails on a refusal and never moves the cursor', async () => {
    const { explorer, moveCursor } = makeExplorer(refused('cannot start'))
    await navigateToFileInPane(explorer, 'left', '/tmp', 'a.txt')
    expect(moveCursor).not.toHaveBeenCalled()
  })
})
