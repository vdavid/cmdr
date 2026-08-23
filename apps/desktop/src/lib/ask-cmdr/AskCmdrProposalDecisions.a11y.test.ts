/**
 * Tier 3 a11y tests for `AskCmdrProposalDecisions.svelte`.
 *
 * A record of what the user answered, so the whole content has to be readable in order: a
 * screen reader gets the sentence, then the path it refers to, then what the run did. The
 * path sits in its own element for the ellipsis and the tooltip, and the risk that buys is a
 * decorative element that reads as nothing.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrProposalDecisions from './AskCmdrProposalDecisions.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { ProposalDecision } from '$lib/tauri-commands'

function mountDecisions(decisions: ProposalDecision[]): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrProposalDecisions, { target, props: { decisions } })
  return target
}

const rejected: ProposalDecision = {
  verb: 'trash',
  what: '/Users/dana/Downloads/*.dmg',
  ops: 12,
  outcome: { kind: 'rejected' },
}

describe('AskCmdrProposalDecisions a11y', () => {
  it('a rejected suggestion has no a11y violations', async () => {
    const target = mountDecisions([rejected])
    await tick()
    await expectNoA11yViolations(target)
  })

  it('an approved run, result line included, has no a11y violations', async () => {
    const target = mountDecisions([
      rejected,
      {
        verb: 'move',
        what: '/Users/dana/Desktop/*.png',
        ops: 3,
        outcome: { kind: 'ran', done: 2, skipped: 1, failed: 0 },
      },
    ])
    await tick()
    await expectNoA11yViolations(target)
  })
})
