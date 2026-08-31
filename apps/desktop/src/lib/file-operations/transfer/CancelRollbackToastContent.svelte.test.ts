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
    render({ headline: 'Removed the 3 items Cmdr had written.', leftBehind: null, reasons: [], level: 'success' })
    expect(target.querySelector('.headline')?.textContent.trim()).toBe('Removed the 3 items Cmdr had written.')
    expect(target.querySelector('.left-behind')).toBeNull()
    expect(target.querySelector('ul')).toBeNull()
  })

  it('gives each leftover reason its own line', () => {
    render({
      headline: 'Removed 9 items.',
      leftBehind: "Cmdr leaves alone anything it isn't sure about, so these stayed where they are:",
      reasons: [
        'Left notes.md alone: it changed after Cmdr put it there.',
        'Left 3 folders alone: they have something in them now.',
      ],
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
      leftBehind: "Cmdr leaves alone anything it isn't sure about, so these stayed where they are:",
      reasons: ['Left notes.md alone: it changed after Cmdr put it there.'],
      level: 'info',
    })
    const rendered = target.textContent
    expect(rendered.indexOf('leaves alone')).toBeGreaterThan(rendered.indexOf('Removed 9 items'))
    expect(rendered.indexOf('leaves alone')).toBeLessThan(rendered.indexOf('Left notes.md'))
  })

  it('opens on the explanation when the reversal undid nothing', () => {
    render({
      headline: null,
      leftBehind: "Cmdr leaves alone anything it isn't sure about, so these stayed where they are:",
      reasons: ['Left 2 items alone: they changed after Cmdr put them there.'],
      level: 'info',
    })
    expect(target.querySelector('.headline')).toBeNull()
    expect(target.querySelector('.left-behind')).not.toBeNull()
    expect(lines()).toHaveLength(1)
  })
})
