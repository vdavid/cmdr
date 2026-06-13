/**
 * Behavior tests for the `favorites.add` command handler: it favorites the focused pane's
 * current directory via the `addFavorite` wrapper.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/downloads/go-to-latest', () => ({ goToLatestDownload: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({ addFavorite: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPanePath: vi.fn(() => '/Users/me/Documents'),
}))
vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn() }))

import { miscHandlers } from './misc-handlers'
import type { CommandHandlerContext } from './types'
import { addFavorite } from '$lib/tauri-commands'
import { getFocusedPanePath } from '$lib/file-explorer/pane/focused-pane-reads'
import { addToast } from '$lib/ui/toast'

// The handler ignores its context (it reads the focused-pane path directly), but the
// `CommandHandler` type still passes one; supply a stub so the call typechecks.
const hctx = { explorerRef: undefined, ctx: {}, dispatchArgs: undefined } as unknown as CommandHandlerContext
const runFavoritesAdd = () => (miscHandlers['favorites.add'] as (hctx: CommandHandlerContext) => Promise<void>)(hctx)

describe('favorites.add handler', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(getFocusedPanePath).mockReturnValue('/Users/me/Documents')
    vi.mocked(addFavorite).mockResolvedValue(undefined)
  })

  it('favorites the focused pane current dir with a null name', async () => {
    await runFavoritesAdd()
    expect(addFavorite).toHaveBeenCalledWith('/Users/me/Documents', null)
  })

  it('shows a success toast naming the folder', async () => {
    await runFavoritesAdd()
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('Documents'), { level: 'success' })
  })

  it('does nothing when there is no focused-pane path', async () => {
    vi.mocked(getFocusedPanePath).mockReturnValue('')
    await runFavoritesAdd()
    expect(addFavorite).not.toHaveBeenCalled()
  })

  it('shows an error toast when the add fails', async () => {
    vi.mocked(addFavorite).mockRejectedValueOnce(new Error('IPC down'))
    await runFavoritesAdd()
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining("Couldn't"), { level: 'error' })
  })
})
