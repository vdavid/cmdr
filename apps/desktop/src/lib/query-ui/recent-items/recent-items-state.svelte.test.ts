// Search and Selection each own a recents store; a load failure has to say whose it was.

import { describe, expect, it, vi } from 'vitest'

const { warnByCategory } = vi.hoisted(() => ({ warnByCategory: new Map<string, ReturnType<typeof vi.fn>>() }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: (feature: string) => {
    const warn = vi.fn()
    warnByCategory.set(feature, warn)
    return { debug: vi.fn(), info: vi.fn(), warn, error: vi.fn() }
  },
}))

import { createRecentItemsState } from './recent-items-state.svelte'

describe('createRecentItemsState', () => {
  it('logs a failed load under the owning dialog’s category', async () => {
    const searchStore = createRecentItemsState<string>({
      logCategory: 'search',
      getRecent: () => Promise.reject(new Error('db locked')),
    })
    const selectionStore = createRecentItemsState<string>({
      logCategory: 'selection',
      getRecent: () => Promise.reject(new Error('db locked')),
    })

    await searchStore.load()
    await selectionStore.load()

    expect(warnByCategory.get('search')).toHaveBeenCalledOnce()
    expect(warnByCategory.get('selection')).toHaveBeenCalledOnce()
  })
})
