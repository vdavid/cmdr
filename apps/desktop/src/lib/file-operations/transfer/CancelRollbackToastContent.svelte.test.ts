/**
 * The toast after a cancelled transfer's reversal.
 *
 * What's pinned here is the STACKING: a leftover reason must render as its own
 * line, never folded into the headline, and the expectation-setting line must
 * come before the reasons rather than after them. `cancel-rollback-toast.test.ts`
 * owns the wording itself.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import CancelRollbackToastContent from './CancelRollbackToastContent.svelte'
import type { CancelRollbackReadout } from './cancel-rollback-toast'

let target: HTMLElement

function render(readout: CancelRollbackReadout): void {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(CancelRollbackToastContent, { target, props: { readout } })
  flushSync()
}

function lines(): string[] {
  return [...target.querySelectorAll('li')].map((item) => item.textContent.trim())
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('CancelRollbackToastContent', () => {
  it('shows a clean reversal as one line, with nothing to explain', () => {
    render({
      headline: 'Removed the 3 items Cmdr had written.',
      leftBehind: null,
      reasons: [],
      staged: null,
      level: 'success',
    })
    expect(target.querySelector('.headline')?.textContent.trim()).toBe('Removed the 3 items Cmdr had written.')
    expect(target.querySelector('.left-behind')).toBeNull()
    expect(target.querySelector('ul')).toBeNull()
  })

  it('gives each leftover reason its own line', () => {
    render({
      headline: 'Removed 9 items.',
      leftBehind: "Cmdr skips anything it isn't sure about, so these stayed where they are:",
      reasons: [
        'Left notes.md alone: it changed after Cmdr put it there.',
        'Left 3 folders alone: they have something in them now.',
      ],
      staged: null,
      level: 'info',
    })
    expect(lines()).toEqual([
      'Left notes.md alone: it changed after Cmdr put it there.',
      'Left 3 folders alone: they have something in them now.',
    ])
  })

  it('sets the expectation BEFORE the reasons, so leftovers read as care rather than as a shortfall', () => {
    render({
      headline: 'Removed 9 items.',
      leftBehind: "Cmdr skips anything it isn't sure about, so these stayed where they are:",
      reasons: ['Left notes.md alone: it changed after Cmdr put it there.'],
      staged: null,
      level: 'info',
    })
    const rendered = target.textContent
    expect(rendered.indexOf('Cmdr skips')).toBeGreaterThan(rendered.indexOf('Removed 9 items'))
    expect(rendered.indexOf('Cmdr skips')).toBeLessThan(rendered.indexOf('Left notes.md'))
  })

  it('opens on the explanation when the reversal undid nothing', () => {
    render({
      headline: null,
      leftBehind: "Cmdr skips anything it isn't sure about, so these stayed where they are:",
      reasons: ['Left 2 items alone: they changed after Cmdr put them there.'],
      staged: null,
      level: 'info',
    })
    expect(target.querySelector('.headline')).toBeNull()
    expect(target.querySelector('.left-behind')).not.toBeNull()
    expect(lines()).toHaveLength(1)
  })

  it("puts Cmdr's own leftover under the reasons, not among them", () => {
    // The bulleted reasons sit under "Cmdr skips anything it isn't sure about",
    // which is Cmdr protecting the user's files. A scratch file the destination
    // wouldn't release is a different kind of news and must not borrow that
    // framing by joining the list.
    render({
      headline: 'Removed 4 items.',
      leftBehind: null,
      reasons: [],
      staged:
        "Couldn't remove holiday.jpg.cmdr-tmp-4d1f9c, an unfinished copy left at the destination. " +
        "It's safe to delete, and Cmdr clears it on a later transfer there.",
      level: 'warn',
    })
    expect(target.querySelector('ul')).toBeNull()
    expect(target.querySelector('.staged')?.textContent).toContain('holiday.jpg.cmdr-tmp-4d1f9c')
    const rendered = target.textContent
    expect(rendered.indexOf("Couldn't remove")).toBeGreaterThan(rendered.indexOf('Removed 4 items'))
  })
})
