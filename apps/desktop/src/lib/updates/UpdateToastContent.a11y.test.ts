/**
 * Tier 3 a11y tests for `UpdateToastContent.svelte`.
 *
 * Toast body with a headline, a detail line, two buttons (Restart now / Later), and an optional
 * version row. The row is the one part with an a11y shape of its own (`role="img"` plus a label,
 * so the arrow isn't read as a bare symbol), so it gets a state of its own alongside the default.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import UpdateToastContent from './UpdateToastContent.svelte'
import { updateState } from './update-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(() => Promise.resolve()),
}))

async function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(UpdateToastContent, { target, props: {} })
  await tick()
  return target
}

describe('UpdateToastContent a11y', () => {
  afterEach(() => {
    updateState.previousVersion = null
    updateState.nextVersion = null
  })

  it('default render has no a11y violations', async () => {
    await expectNoA11yViolations(await render())
  })

  it('has no a11y violations with the version row showing', async () => {
    updateState.previousVersion = '0.28.3'
    updateState.nextVersion = '0.29.0'
    await expectNoA11yViolations(await render())
  })
})
