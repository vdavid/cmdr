import { describe, it, expect, vi } from 'vitest'

import { onListingGone, throwListingLookupError } from './listing-gone'

describe('throwListingLookupError', () => {
  it('tells every listener which listing is gone, then throws the typed refusal', () => {
    const heard = vi.fn()
    const unsubscribe = onListingGone(heard)

    expect(() => throwListingLookupError({ type: 'gone', listingId: 'abc' })).toThrow(/abc/)
    expect(heard).toHaveBeenCalledWith('abc')

    unsubscribe()
    expect(() => throwListingLookupError({ type: 'gone', listingId: 'def' })).toThrow()
    expect(heard).toHaveBeenCalledTimes(1)
  })
})
