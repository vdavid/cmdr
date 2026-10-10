/**
 * Tier 3 a11y tests for `FavoriteRefusalToastContent.svelte`.
 *
 * The toast a favorite pick raises when the pane can't go there: a sentence, plus
 * one action button when there's a way out. Both shapes are covered.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FavoriteRefusalToastContent from './FavoriteRefusalToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

async function render(actionLabel: string | null): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FavoriteRefusalToastContent, {
    target,
    props: {
      toastId: 'favorite-refusal',
      message: 'Cmdr doesn’t know how to reach “naspi on nas.local” anymore.',
      actionLabel,
      onAction: vi.fn(),
    },
  })
  await tick()
  return target
}

describe('FavoriteRefusalToastContent a11y', () => {
  it('with an action has no a11y violations', async () => {
    await expectNoA11yViolations(await render('Show servers'))
  })

  it('without an action has no a11y violations', async () => {
    await expectNoA11yViolations(await render(null))
  })
})
