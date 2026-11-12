/**
 * The add's two failure toasts: a slow volume (the gate's typed `timedOut`) reads differently from
 * every other failure, which keeps the generic "Couldn't add" words (cmdr-reports#40).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/tauri-commands/index.js', () => ({ addFavorite: vi.fn() }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))

import { addFavorite } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { AddFavoriteFailure } from './add-favorite-error'
import { addFavoriteFolder } from './add-favorite-folder'

describe('addFavoriteFolder failures', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('says the folder is slow to respond when the gate timed out', async () => {
    vi.mocked(addFavorite).mockRejectedValueOnce(new AddFavoriteFailure({ type: 'timedOut' }))
    await addFavoriteFolder('/Volumes/testshare/Photos')
    expect(addToast).toHaveBeenCalledWith(
      'That folder is slow to respond right now, so it’s not in your favorites yet. Try again in a moment?',
      { level: 'warn' },
    )
  })

  it('keeps the generic words for a refusal', async () => {
    vi.mocked(addFavorite).mockRejectedValueOnce(new AddFavoriteFailure({ type: 'notAPlace' }))
    await addFavoriteFolder('search-results://8f3a1c')
    expect(addToast).toHaveBeenCalledWith('Couldn’t add that folder to favorites. Try again?', { level: 'error' })
  })

  it('keeps the generic words for an untyped failure', async () => {
    vi.mocked(addFavorite).mockRejectedValueOnce(new Error('IPC down'))
    await addFavoriteFolder('/Users/me/Docs')
    expect(addToast).toHaveBeenCalledWith('Couldn’t add that folder to favorites. Try again?', { level: 'error' })
  })
})
