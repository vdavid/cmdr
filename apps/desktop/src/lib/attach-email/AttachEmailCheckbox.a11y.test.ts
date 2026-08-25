/**
 * Tier 3 a11y tests for `AttachEmailCheckbox.svelte`.
 *
 * The shared "Attach my email" opt-in rendered by the crash-report, error-report, and
 * feedback dialogs. It has two shapes, both covered here: reusing the address on file,
 * and collecting one through the inline field that a tick reveals. The field carries an
 * accessible name of its own, and its validation message is wired to it through
 * `aria-describedby` + `aria-invalid`.
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

/**
 * Mount and settle. The await matters: Ark's checkbox machine only starts once the
 * mount's effects have flushed, and a synthetic click before that toggles the DOM input
 * without ever reaching the binding.
 */
async function render(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AttachEmailCheckbox, { target, props: { email: createAttachEmail() } })
  await tick()
  return target
}

function checkboxIn(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector('input[type="checkbox"]')
}

function emailInputIn(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector('input[type="email"]')
}

/** Tick the box the way a user does: the primitive syncs off the input's real click. */
async function tickBox(target: HTMLElement) {
  checkboxIn(target)?.click()
  await tick()
}

/** Type into the revealed field, driving the same `input` event the browser fires. */
async function typeEmail(target: HTMLElement, value: string) {
  const input = emailInputIn(target)
  if (!input) throw new Error('email input missing')
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
}

describe('AttachEmailCheckbox a11y', () => {
  it('has no violations when an email is on file', async () => {
    mockEmail = 'alex@example.com'
    const target = await render()
    await expectNoA11yViolations(target)
  })

  it('names the checkbox with the label, including the address', async () => {
    mockEmail = 'alex@example.com'
    const target = await render()
    expect(checkboxIn(target)).not.toBeNull()
    expect(target.textContent).toContain('Attach my email (alex@example.com) so we can reply')
  })

  it('offers no field when the address on file already answers the question', async () => {
    mockEmail = 'alex@example.com'
    const target = await render()
    await tickBox(target)
    expect(emailInputIn(target)).toBeNull()
  })

  it('still renders, unticked, when no email is on file', async () => {
    mockEmail = ''
    const target = await render()
    const box = checkboxIn(target)
    expect(box).not.toBeNull()
    expect(box?.checked).toBe(false)
    expect(target.textContent).toContain('Attach my email so we can reply')
    // The field is the tick's reward, so the question never gets asked twice.
    expect(emailInputIn(target)).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('reveals a named email field once ticked, with no violations', async () => {
    mockEmail = ''
    const target = await render()
    await tickBox(target)
    const input = emailInputIn(target)
    expect(input).not.toBeNull()
    expect(input?.getAttribute('aria-label')).toBe('Your email address')
    expect(input?.getAttribute('aria-invalid')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('leaves the field unflagged while it is empty', async () => {
    mockEmail = ''
    const target = await render()
    await tickBox(target)
    expect(target.textContent).not.toContain("doesn't look like an email address")
    expect(emailInputIn(target)?.getAttribute('aria-invalid')).toBeNull()
  })

  it('wires the validation message to the field it describes', async () => {
    mockEmail = ''
    const target = await render()
    await tickBox(target)
    await typeEmail(target, 'foo')

    const input = emailInputIn(target)
    expect(input?.getAttribute('aria-invalid')).toBe('true')
    const describedBy = input?.getAttribute('aria-describedby')
    expect(describedBy).toBeTruthy()
    const message = describedBy ? target.querySelector(`#${CSS.escape(describedBy)}`) : null
    expect(message?.textContent).toContain("doesn't look like an email address")
    await expectNoA11yViolations(target)
  })

  it('clears the flag once the address takes shape', async () => {
    mockEmail = ''
    const target = await render()
    await tickBox(target)
    await typeEmail(target, 'foo')
    await typeEmail(target, 'foo@example.com')
    const input = emailInputIn(target)
    expect(input?.getAttribute('aria-invalid')).toBeNull()
    expect(input?.getAttribute('aria-describedby')).toBeNull()
  })
})
