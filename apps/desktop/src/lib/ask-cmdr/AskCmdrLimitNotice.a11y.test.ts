/**
 * Tier 3 a11y tests for `AskCmdrLimitNotice.svelte`.
 *
 * The note under an Ask Cmdr turn that reached a per-answer limit. What this guards is the
 * tinted strip's contrast, the Send feedback link staying a real, named button, and the
 * "Keep going" button being named, for every kind of limit.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { TurnLimit } from '#lib/ipc/bindings.js'

vi.mock('#lib/feedback/feedback-flow.svelte.js', () => ({ openFeedbackDialog: vi.fn() }))

import AskCmdrLimitNotice from './AskCmdrLimitNotice.svelte'

const LIMITS: TurnLimit[] = [
  { kind: 'toolRounds', max: 25, answered: true },
  { kind: 'toolRounds', max: 25, answered: false },
  { kind: 'wallTime', max: 600, answered: false },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('AskCmdrLimitNotice a11y', () => {
  for (const limit of LIMITS) {
    for (const canContinue of [true, false]) {
      it(`${limit.kind} (answered: ${String(limit.answered)}, continue: ${String(canContinue)}) has no violations`, async () => {
        const target = document.createElement('div')
        document.body.appendChild(target)
        mount(AskCmdrLimitNotice, { target, props: { limit, canContinue, onContinue: vi.fn() } })
        await tick()
        await expectNoA11yViolations(target)
      })
    }
  }
})
