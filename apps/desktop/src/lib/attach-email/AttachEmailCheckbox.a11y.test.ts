/**
 * Tier 3 a11y tests for `AttachEmailCheckbox.svelte`.
 *
 * The shared "Attach my email" opt-in rendered by the crash-report, error-report, and
 * feedback dialogs. It hides itself when no contact email is on file, so both shapes are
 * covered: the rendered checkbox (which must carry its label as an accessible name) and
 * the empty render.
 */

import { describe, it, vi, expect, beforeAll, afterAll } from 'vitest'
import { mount, tick } from 'svelte'
import AttachEmailCheckbox from './AttachEmailCheckbox.svelte'
import { createAttachEmail } from './attach-email.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { _setLocaleForTests } from '$lib/intl/locale'

let mockEmail = ''

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((id: string) => (id === 'analytics.email' ? mockEmail : false)),
  setSetting: vi.fn(),
}))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AttachEmailCheckbox, { target, props: { email: createAttachEmail() } })
  return target
}

describe('AttachEmailCheckbox a11y', () => {
  it('has no violations when an email is on file', async () => {
    mockEmail = 'alex@example.com'
    const target = render()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('names the checkbox with the label, including the address', async () => {
    mockEmail = 'alex@example.com'
    const target = render()
    await tick()
    const box = target.querySelector('[role="checkbox"], input[type="checkbox"]')
    expect(box).not.toBeNull()
    expect(target.textContent).toContain('Attach my email (alex@example.com) so we can reply')
  })

  it('renders nothing, and no violations, when no email is on file', async () => {
    mockEmail = ''
    const target = render()
    await tick()
    expect(target.querySelector('[role="checkbox"], input[type="checkbox"]')).toBeNull()
    await expectNoA11yViolations(target)
  })
})
