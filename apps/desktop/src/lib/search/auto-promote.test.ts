import { describe, it, expect } from 'vitest'
import { armAutoPromote, takeAutoPromote } from './snapshot-promotion'

/**
 * The one-shot arm quick find (⌘⇧F) sets before opening the dialog, so the run it
 * asked for lands in a pane instead of a dialog the user then has to dismiss.
 *
 * One-shot is the whole point: a session where the arm survived would promote the
 * NEXT search too, closing the dialog under a user who opened it with ⌘F and meant
 * to stay there.
 */
describe('auto-promote arm', () => {
  it('answers true once per arm, and false ever after', () => {
    armAutoPromote()
    expect(takeAutoPromote()).toBe(true)
    expect(takeAutoPromote()).toBe(false)
  })

  it('is disarmed until something arms it', () => {
    expect(takeAutoPromote()).toBe(false)
  })
})
