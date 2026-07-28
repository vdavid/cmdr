/**
 * Tier 3 a11y tests for the `TextArea` primitive (`TextInput`'s multi-line sibling).
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import TextArea from './TextArea.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function mountTarget(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

describe('TextArea a11y', () => {
  it('named by ariaLabel has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextArea, { target, props: { value: 'It crashed when I hit F5.', ariaLabel: 'What happened?', rows: 4 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('named by an external <label for> has no a11y violations', async () => {
    const target = mountTarget()
    const label = document.createElement('label')
    label.htmlFor = 'feedback-body'
    label.textContent = 'Your feedback'
    target.appendChild(label)
    mount(TextArea, { target, props: { value: '', id: 'feedback-body', rows: 4 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('read-only, non-resizable state has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextArea, {
      target,
      props: { value: 'Copy failed on 3 files.', readonly: true, resizable: false, ariaLabel: 'Error detail', rows: 6 },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('invalid and disabled states have no a11y violations', async () => {
    const target = mountTarget()
    mount(TextArea, { target, props: { value: 'x', invalid: true, disabled: true, ariaLabel: 'Notes', rows: 3 } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
