/**
 * The line recording what the user answered when the agent suggested something.
 *
 * The load-bearing property is the same one the wake digest has: every WORD comes from the
 * catalog and every NUMBER and PATH from the wire. The backend hands over a verb token, a
 * count, and the group's own display text precisely so ten locales can each say it their own
 * way, and a rendered backend sentence appearing here would be untranslated copy frozen in
 * `main.db`.
 *
 * The other half is honesty about approvals: an approved group can go on to skip every file,
 * so the result line always renders, zeros included.
 */

import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrProposalDecisions from './AskCmdrProposalDecisions.svelte'
import type { ProposalDecision } from '$lib/tauri-commands'

function decision(overrides: Partial<ProposalDecision> = {}): ProposalDecision {
  return {
    verb: 'trash',
    what: '/Users/dana/Downloads/*.dmg',
    ops: 12,
    outcome: { kind: 'rejected' },
    ...overrides,
  }
}

function render(decisions: ProposalDecision[]): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrProposalDecisions, { target, props: { decisions } })
  return target
}

describe('AskCmdrProposalDecisions', () => {
  it('says what the user turned down, in the catalog is words and the wire is numbers', async () => {
    const target = render([decision()])
    await tick()

    expect(target.textContent).toContain('You turned down trashing 12 items')
    expect(target.textContent).toContain('/Users/dana/Downloads/*.dmg')
  })

  it('names the verb the user actually answered about', async () => {
    const target = render([decision({ verb: 'delete', ops: 1 })])
    await tick()

    expect(target.textContent).toContain('You turned down deleting 1 item')
  })

  /** ⚠️ An approval is a claim; this line is what happened. A group can be approved and then
   *  skip every file behind a fingerprint mismatch, and hiding that would leave the user
   *  believing their files moved. */
  it('reports what an approved run actually did, zeros included', async () => {
    const target = render([
      decision({ verb: 'move', ops: 3, outcome: { kind: 'ran', done: 2, skipped: 1, failed: 0 } }),
    ])
    await tick()

    expect(target.textContent).toContain('You approved moving 3 items')
    expect(target.textContent).toContain('2 done, 1 skipped, 0 failed')
  })

  /** A rejection never ran, so there is nothing to report about it. */
  it('says nothing about a run for a suggestion that was turned down', async () => {
    const target = render([decision()])
    await tick()

    expect(target.querySelector('.result')).toBeNull()
  })

  /** The follow-up turn a rejected sweep earns carries the whole sweep, which is the point of
   *  coalescing: one turn, every group the user said no to. */
  it('renders every decision of a sweep answered at once', async () => {
    const target = render([
      decision({ verb: 'trash', what: '/Users/dana/Downloads/*.dmg' }),
      decision({ verb: 'move', what: '/Users/dana/Desktop/*.png', ops: 4 }),
    ])
    await tick()

    expect(target.querySelectorAll('li')).toHaveLength(2)
    expect(target.textContent).toContain('/Users/dana/Desktop/*.png')
  })

  /** A path is attacker-controlled (it comes off the user's disk), so it renders as escaped
   *  plain text and never as markup. */
  it('renders a path as text rather than as markup', async () => {
    const target = render([decision({ what: '<img src=x onerror=alert(1)>' })])
    await tick()

    expect(target.querySelector('img')).toBeNull()
    expect(target.textContent).toContain('<img src=x onerror=alert(1)>')
  })
})
