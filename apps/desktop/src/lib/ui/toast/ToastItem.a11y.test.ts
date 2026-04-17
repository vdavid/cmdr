/**
 * Tier 3 a11y tests for `ToastItem.svelte`.
 *
 * Individual toast frame with a close button. Each level uses the right
 * `role` (`status` for info/success, `alert` for warn/error). Tests
 * cover all levels, both dismissal modes, and string content.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ToastItem from './ToastItem.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ToastItem a11y', () => {
  it('info level (string content, transient) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastItem, {
      target,
      props: {
        id: 't1',
        content: 'Saved your changes',
        level: 'info',
        dismissal: 'transient',
        timeoutMs: 4000,
        ondismiss: vi.fn(),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('success level has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastItem, {
      target,
      props: {
        id: 't2',
        content: 'Files copied successfully',
        level: 'success',
        dismissal: 'transient',
        timeoutMs: 4000,
        ondismiss: vi.fn(),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('warn level (persistent) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastItem, {
      target,
      props: {
        id: 't3',
        content: 'Update available — restart to apply',
        level: 'warn',
        dismissal: 'persistent',
        timeoutMs: 0,
        ondismiss: vi.fn(),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('error level has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToastItem, {
      target,
      props: {
        id: 't4',
        content: 'Could not reach the server',
        level: 'error',
        dismissal: 'transient',
        timeoutMs: 4000,
        ondismiss: vi.fn(),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
