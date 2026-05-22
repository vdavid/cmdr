/**
 * Tier-3 a11y tests for `AiTransparencyStrip.svelte`.
 *
 * Pins that the strip is axe-clean with and without a caveat, and that the disabled "Refine…"
 * button doesn't trip nested-interactive or hidden-content rules.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import AiTransparencyStrip from './AiTransparencyStrip.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof AiTransparencyStrip>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    aiPrompt: 'screenshots from this week',
    caveat: '',
    ...overrides,
  }
}

describe('AiTransparencyStrip a11y', () => {
  it('has no a11y violations with a prompt only', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiTransparencyStrip, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('has no a11y violations with a caveat', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiTransparencyStrip, {
      target,
      props: baseProps({ caveat: "I treated 'big' as larger than 10 MB." }),
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
