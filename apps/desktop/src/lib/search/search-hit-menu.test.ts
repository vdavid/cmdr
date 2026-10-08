/**
 * What a search hit's context menu offers. Every hit is a real file the index
 * walked, so the items that act through the row's path (Share, the tag colors)
 * belong on it in the Search dialog just as they do in a folder.
 */
import { describe, it, expect } from 'vitest'
import { SEARCH_HIT_MENU_FACTS, searchDialogRowMenuFacts } from './search-hit-menu'

describe('search hit context menu facts', () => {
  it('offers Share and the tag colors on any hit', () => {
    expect(SEARCH_HIT_MENU_FACTS).toEqual({ canShare: true, canTag: true })
  })

  it('in the Search dialog, offers Share on a file and on a folder, and favorites only on a folder', () => {
    expect(searchDialogRowMenuFacts({ isDirectory: false })).toEqual({
      canShare: true,
      canTag: true,
      canFavorite: false,
    })
    expect(searchDialogRowMenuFacts({ isDirectory: true })).toEqual({
      canShare: true,
      canTag: true,
      canFavorite: true,
    })
  })
})
