/**
 * The open/close seam for the Acknowledgements dialog. Small, but it's the only thing
 * standing between the `help.acknowledgements` command handler and the `{#if}` in
 * `+page.svelte` — and `acknowledgementsState.open` also feeds that page's
 * "is a modal up?" guard, which suppresses central shortcut dispatch. A trigger stuck
 * open would silently swallow every keyboard shortcut.
 *
 * Same shape as the sibling `whats-new/whats-new-trigger.test.ts`.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { acknowledgementsState, openAcknowledgements, closeAcknowledgements } from './acknowledgements-trigger.svelte'

describe('acknowledgements trigger', () => {
  beforeEach(() => {
    closeAcknowledgements()
  })

  it('starts closed', () => {
    expect(acknowledgementsState.open).toBe(false)
  })

  it('opens on the command-handler seam', () => {
    openAcknowledgements()
    expect(acknowledgementsState.open).toBe(true)
  })

  it('closes on the dialog seam', () => {
    openAcknowledgements()
    closeAcknowledgements()
    expect(acknowledgementsState.open).toBe(false)
  })

  it('is idempotent both ways (a menu/palette double-fire is harmless)', () => {
    openAcknowledgements()
    openAcknowledgements()
    expect(acknowledgementsState.open).toBe(true)

    closeAcknowledgements()
    closeAcknowledgements()
    expect(acknowledgementsState.open).toBe(false)
  })
})
