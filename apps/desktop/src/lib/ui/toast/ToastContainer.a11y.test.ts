/**
 * Tier 3 a11y tests for `ToastContainer.svelte`.
 *
 * Empty and populated states, with multiple toast levels. The container
 * uses `aria-live="polite"` so screen readers announce new toasts.
 */

import { describe, it, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import ToastContainer from './ToastContainer.svelte'
import { addToast, clearAllToasts } from './toast-store.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ToastContainer a11y', () => {
  beforeEach(() => {
    clearAllToasts()
  })

  it('empty (no toasts) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastContainer, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a single info toast has no a11y violations', async () => {
    addToast('Your file has been copied', { level: 'info' })
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastContainer, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with mixed toast levels has no a11y violations', async () => {
    addToast('Saved', { level: 'success' })
    addToast('Watch out: slow mount', { level: 'warn' })
    addToast('Connection lost', { level: 'error' })
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastContainer, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
