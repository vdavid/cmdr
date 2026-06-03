import { describe, it, expect, vi, beforeEach } from 'vitest'
import { navigateToDirInPane, navigateToFileInPane } from './navigate-and-select'
import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'

function makeExplorer(navResult: string | Promise<void>) {
  const navigateToPath = vi.fn(() => navResult)
  const moveCursor = vi.fn(() => Promise.resolve())
  const explorer = { navigateToPath, moveCursor } as unknown as ExplorerAPI
  return { explorer, navigateToPath, moveCursor }
}

describe('navigateToDirInPane', () => {
  it('navigates to the dir and never moves the cursor', async () => {
    const { explorer, navigateToPath, moveCursor } = makeExplorer(Promise.resolve())
    await navigateToDirInPane(explorer, 'left', '/tmp/here')
    expect(navigateToPath).toHaveBeenCalledWith('left', '/tmp/here')
    expect(moveCursor).not.toHaveBeenCalled()
  })

  it('bails on the sync-error string without throwing', async () => {
    const { explorer, moveCursor } = makeExplorer('snapshot pane on a missing volume')
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
    const { explorer, navigateToPath, moveCursor } = makeExplorer(Promise.resolve())
    await navigateToFileInPane(explorer, 'left', '/tmp', 'a.txt')
    expect(navigateToPath).toHaveBeenCalledWith('left', '/tmp')
    expect(moveCursor).toHaveBeenCalledWith('left', 'a.txt')
  })

  it('awaits the listing before moving the cursor', async () => {
    let resolveListing!: () => void
    const listing = new Promise<void>((resolve) => {
      resolveListing = resolve
    })
    const navigateToPath = vi.fn(() => listing)
    const moveCursor = vi.fn(() => {
      moveCursorCalls.push(['called'])
      return Promise.resolve()
    })
    const explorer = { navigateToPath, moveCursor } as unknown as ExplorerAPI

    const promise = navigateToFileInPane(explorer, 'left', '/tmp', 'a.txt')
    // Cursor must NOT move before the listing settles.
    expect(moveCursorCalls).toHaveLength(0)
    resolveListing()
    await promise
    expect(moveCursorCalls).toHaveLength(1)
  })

  it('bails on the sync-error string and never moves the cursor', async () => {
    const { explorer, moveCursor } = makeExplorer('cannot start')
    await navigateToFileInPane(explorer, 'left', '/tmp', 'a.txt')
    expect(moveCursor).not.toHaveBeenCalled()
  })
})
