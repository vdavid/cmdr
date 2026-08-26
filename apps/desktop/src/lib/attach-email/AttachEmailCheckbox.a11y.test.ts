/**
 * Tier 3 a11y tests for `AttachEmailCheckbox.svelte`.
 *
 * The shared "Attach my email" opt-in rendered by the crash-report, error-report, and
 * feedback dialogs. It has two shapes, both covered here: reusing the address on file
 * (whose label carries an inline "change" link into Settings), and collecting one through
 * the inline field that a tick reveals. The field carries an accessible name of its own,
 * and its validation message is wired to it through `aria-describedby` + `aria-invalid`.
 * The shape follows `analytics.email` live, in both directions, so that is covered too.
 */

import { describe, it, vi, expect, beforeAll, beforeEach, afterAll } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import AttachEmailFixture from './attach-email-fixture.svelte'
import type { AttachEmail } from './attach-email.svelte'
import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { _setLocaleForTests } from '$lib/intl/locale'

let mockEmail = ''

/** Live listeners on `analytics.email`, so a test can play the Settings window's part. */
const emailListeners = new Set<(value: string) => void>()

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((id: string) => (id === 'analytics.email' ? mockEmail : false)),
  setSetting: vi.fn(),
  onSpecificSettingChange: (id: string, listener: (value: string) => void) => {
    if (id !== 'analytics.email') return () => {}
    emailListeners.add(listener)
    return () => emailListeners.delete(listener)
  },
}))

vi.mock('$lib/settings/settings-window', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

beforeEach(() => {
  vi.mocked(openSettingsWindow).mockClear()
  emailListeners.clear()
})

/** The mounted fixture, its state, and the handle that unmounts it. */
interface Mounted {
  target: HTMLElement
  email: AttachEmail
  unmountFixture: () => Promise<void>
}

/**
 * Mount and settle. The await matters: Ark's checkbox machine only starts once the
 * mount's effects have flushed, and a synthetic click before that toggles the DOM input
 * without ever reaching the binding.
 */
async function renderFixture(): Promise<Mounted> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  let email: AttachEmail | undefined
  const component = mount(AttachEmailFixture, {
    target,
    props: {
      onState: (state: AttachEmail) => {
        email = state
      },
    },
  })
  await tick()
  if (!email) throw new Error('state was not created')
  return { target, email, unmountFixture: () => unmount(component) }
}

/** The old shape, for the tests that only look at the DOM. */
async function render(): Promise<HTMLElement> {
  return (await renderFixture()).target
}

/** Play the Settings window: the user edits `analytics.email` while the dialog stays up. */
async function setContactEmailFromSettings(value: string) {
  mockEmail = value
  for (const listener of [...emailListeners]) listener(value)
  await tick()
}

/** Rendered text with HTML's own whitespace collapsing applied, so markup layout can't fail an assertion. */
function flatText(target: HTMLElement): string {
  return target.textContent.replace(/\s+/g, ' ').trim()
}

function checkboxIn(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector('input[type="checkbox"]')
}

function emailInputIn(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector('input[type="email"]')
}

/** The inline "change" link inside the checkbox label. */
function changeLinkIn(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector('button.link-button')
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
    expect(flatText(target)).toContain('Attach my email address (alex@example.com – change) so you can follow up')
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
    expect(flatText(target)).toContain('Attach my email address so you can follow up')
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
    expect(flatText(target)).not.toContain("doesn't look like an email address")
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

describe('AttachEmailCheckbox change link', () => {
  it('offers the link only when there is an address to change', async () => {
    mockEmail = ''
    const target = await render()
    expect(changeLinkIn(target)).toBeNull()
  })

  it('names the link and puts it in the tab order', async () => {
    mockEmail = 'alex@example.com'
    const target = await render()
    const link = changeLinkIn(target)
    expect(link).not.toBeNull()
    expect(link?.textContent.trim()).toBe('change')
    // A real `<button>`: reachable by Tab and activated by Enter/Space, no tabindex games.
    expect(link?.tabIndex).toBe(0)
    expect(link?.disabled).toBe(false)
    await expectNoA11yViolations(target)
  })

  it('deep-links Settings to the contact-email row', async () => {
    mockEmail = 'alex@example.com'
    const target = await render()
    changeLinkIn(target)?.click()
    // The module is imported lazily, so the call lands a microtask or two later.
    await vi.waitFor(() => {
      expect(vi.mocked(openSettingsWindow)).toHaveBeenCalledWith(
        'attach-email',
        ['Updates & privacy'],
        settingAnchorId('analytics.email'),
      )
    })
  })

  it('leaves the tick alone when the link inside the label is clicked', async () => {
    mockEmail = 'alex@example.com'
    const { target, email } = await renderFixture()
    expect(email.attach).toBe(false)

    changeLinkIn(target)?.click()
    await tick()

    // The link sits inside Ark's `<label>` root, which would otherwise forward the click
    // to the checkbox and tick it on the way to Settings.
    expect(email.attach).toBe(false)
    expect(checkboxIn(target)?.checked).toBe(false)
  })
})

describe('AttachEmailCheckbox following analytics.email live', () => {
  it('swaps the collect field for the on-file label when Settings gains an address', async () => {
    mockEmail = ''
    const { target } = await renderFixture()
    await tickBox(target)
    expect(emailInputIn(target)).not.toBeNull()

    await setContactEmailFromSettings('alex@example.com')

    expect(emailInputIn(target)).toBeNull()
    expect(flatText(target)).toContain('Attach my email address (alex@example.com – change) so you can follow up')
    await expectNoA11yViolations(target)
  })

  it('reveals the collect field when the address on file is cleared under a ticked box', async () => {
    mockEmail = 'alex@example.com'
    const { target } = await renderFixture()
    await tickBox(target)
    expect(emailInputIn(target)).toBeNull()

    await setContactEmailFromSettings('')

    const input = emailInputIn(target)
    expect(input).not.toBeNull()
    expect(input?.getAttribute('aria-label')).toBe('Your email address')
    expect(input?.value).toBe('')
    expect(flatText(target)).toContain('Attach my email address so you can follow up')
    expect(changeLinkIn(target)).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('names the new address after the user edits it in Settings', async () => {
    mockEmail = 'old@example.com'
    const { target } = await renderFixture()

    await setContactEmailFromSettings('new@example.com')

    expect(flatText(target)).toContain('new@example.com')
    expect(flatText(target)).not.toContain('old@example.com')
  })

  it('stops following once the dialog closes', async () => {
    mockEmail = 'alex@example.com'
    const { unmountFixture } = await renderFixture()
    expect(emailListeners.size).toBe(1)

    await unmountFixture()

    expect(emailListeners.size).toBe(0)
  })
})
