/**
 * The seeding mechanism's one job: a preview must never leave the app
 * half-seeded. The undo is derived from the patch's own keys, so these pin that
 * the derivation is exact — every field the patch touched comes back, and no
 * field it didn't touch is invented.
 */

import { describe, expect, it } from 'vitest'
import { seedStore } from './store-seeding'

describe('seedStore', () => {
  it('applies the patch and restores every field it touched', () => {
    const store = { open: false, entries: ['real'], loading: false }
    const restore = seedStore(store, { open: true, entries: ['fixture'] })

    expect(store).toEqual({ open: true, entries: ['fixture'], loading: false })
    restore()
    expect(store).toEqual({ open: false, entries: ['real'], loading: false })
  })

  it('restores the ORIGINAL value, not a default, and leaves untouched fields alone', () => {
    const store = { open: false, initialNote: 'a note the app put there', unrelated: 7 }
    const restore = seedStore(store, { open: true, initialNote: 'fixture note' })
    store.unrelated = 9

    restore()
    expect(store.initialNote).toBe('a note the app put there')
    expect(store.unrelated).toBe(9)
  })

  it('restores the same reference, so a store holding live objects is put back intact', () => {
    const live = { id: 1 }
    const store: { review: { id: number } | null } = { review: live }
    const restore = seedStore(store, { review: { id: 2 } })

    restore()
    expect(store.review).toBe(live)
  })

  it('is idempotent enough to run twice (a double teardown restores the same snapshot)', () => {
    const store = { open: false }
    const restore = seedStore(store, { open: true })

    restore()
    restore()
    expect(store.open).toBe(false)
  })
})
