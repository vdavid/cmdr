/**
 * Tier 3 a11y tests for `Spinner.svelte`.
 *
 * The shared loading spinner. Decorative by default (`aria-hidden`, for the common case where
 * adjacent text already says "Loading…"); when it's the sole loading signal the caller passes
 * `label`, which becomes an `aria-label` on a `role="status"`. axe confirms both shapes are clean.
 */
import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import Spinner from './Spinner.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

async function renderSpinner(props: ComponentProps<typeof Spinner>): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Spinner, { target, props })
  await tick()
  return target
}

describe('Spinner a11y', () => {
  it('decorative spinner (default) has no a11y violations', async () => {
    for (const size of ['sm', 'md', 'lg'] as const) {
      const target = await renderSpinner({ size })
      await expectNoA11yViolations(target)
    }
  })

  it('labeled spinner (sole loading indicator) has no a11y violations', async () => {
    const target = await renderSpinner({ size: 'sm', label: 'Loading suggestions' })
    await expectNoA11yViolations(target)
  })
})
