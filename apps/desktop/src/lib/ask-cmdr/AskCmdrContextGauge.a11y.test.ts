/**
 * Tier 3 a11y tests for `AskCmdrContextGauge.svelte`, the rail's context-usage gauge.
 *
 * The gauge is an ARIA meter: it must carry both a NAME and a value, or assistive tech
 * announces a bare number. All three visible states are checked, since each renders a
 * different fill and the "set aside" one is the state a user most needs read out.
 */

import { flushSync, mount } from 'svelte'
import { beforeAll, describe, it } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { expectNoA11yViolations } from '$lib/test-a11y'
import AskCmdrContextGauge from './AskCmdrContextGauge.svelte'
import type { ContextUsage } from './ask-cmdr-context-usage'

beforeAll(() => {
  _setLocaleForTests('en-US')
})

async function expectClean(usage: ContextUsage): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrContextGauge, { target, props: { usage } })
  flushSync()
  await expectNoA11yViolations(target)
  target.remove()
}

describe('AskCmdrContextGauge a11y', () => {
  it('a calm gauge has no a11y violations', async () => {
    await expectClean({ estimatedTokens: 31_200, budgetTokens: 60_000, elidedResults: 0 })
  })

  it('a filling gauge has no a11y violations', async () => {
    await expectClean({ estimatedTokens: 50_000, budgetTokens: 60_000, elidedResults: 0 })
  })

  it('a set-aside gauge has no a11y violations', async () => {
    await expectClean({ estimatedTokens: 59_000, budgetTokens: 60_000, elidedResults: 3 })
  })
})
