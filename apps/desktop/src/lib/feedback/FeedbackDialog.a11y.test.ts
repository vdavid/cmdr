/**
 * Tier 3 a11y + behavior tests for `FeedbackDialog.svelte`.
 *
 * The dialog exposes a textarea, an optional attach-email checkbox, two external
 * links, and send/cancel actions. The `sendFeedback` IPC wrapper is mocked so the
 * tests run deterministically.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import FeedbackDialog from './FeedbackDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { closeFeedbackDialog, feedbackFlow } from './feedback-flow.svelte'
import { GITHUB_ISSUES_URL, BOOK_A_CALL_URL } from '$lib/beta-links'

const sendFeedbackMock = vi.fn<(text: string, email?: string) => Promise<{ kind: string }>>()
const openExternalUrlMock = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve())

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  sendFeedback: (text: string, email?: string) => sendFeedbackMock(text, email),
  openExternalUrl: (url: string) => openExternalUrlMock(url),
}))

// Settings are mocked per-test via these refs so the email-on-file and sticky-default
// states can vary. Defaults: no email on file, attach-default off.
let mockEmail = ''
let mockAttachDefault = false
const setSettingMock = vi.fn()
vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((id: string) => (id === 'analytics.email' ? mockEmail : mockAttachDefault)),
  setSetting: (id: string, value: unknown) => {
    setSettingMock(id, value)
  },
}))

const addToastMock = vi.fn<(...args: unknown[]) => void>()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToastMock(...args)
  },
}))

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FeedbackDialog, { target, props: {} })
  await tick()
  return target
}

function findButton(target: HTMLElement, text: string): HTMLButtonElement | undefined {
  return Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === text)
}

async function typeFeedback(target: HTMLElement, text: string): Promise<HTMLTextAreaElement> {
  const textarea = target.querySelector('textarea')
  if (!textarea) throw new Error('textarea missing')
  textarea.value = text
  textarea.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  return textarea
}

describe('FeedbackDialog', () => {
  beforeEach(() => {
    closeFeedbackDialog()
    mockEmail = ''
    mockAttachDefault = false
    sendFeedbackMock.mockReset()
    sendFeedbackMock.mockResolvedValue({ kind: 'sent' })
    openExternalUrlMock.mockClear()
    addToastMock.mockClear()
    setSettingMock.mockClear()
  })

  it('default render has no a11y violations', async () => {
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })

  it('disables Send while the textarea is empty, enables it once there is text', async () => {
    const target = await mountDialog()
    const sendButton = findButton(target, 'Send feedback')
    expect(sendButton?.disabled).toBe(true)

    await typeFeedback(target, 'Loving the app so far!')
    expect(sendButton?.disabled).toBe(false)
  })

  it('keeps Send disabled for whitespace-only text', async () => {
    const target = await mountDialog()
    await typeFeedback(target, '   \n\t')
    expect(findButton(target, 'Send feedback')?.disabled).toBe(true)
  })

  it('disables Send when the code-point count exceeds the hard cap', async () => {
    const target = await mountDialog()
    // 100 001 code points of emoji (200 002 UTF-16 units): naive `.length` would
    // misreport; the cap must fire on code points like the Rust + server validators.
    await typeFeedback(target, '\u{1F680}'.repeat(100_001))
    expect(findButton(target, 'Send feedback')?.disabled).toBe(true)
  })

  it('shows the counter past the soft-warn threshold using code-point counts', async () => {
    const target = await mountDialog()
    await typeFeedback(target, '\u{1F680}'.repeat(50_001))
    const counter = target.querySelector('.counter')
    expect(counter).not.toBeNull()
    expect(counter?.textContent).toContain((50_001).toLocaleString('en-US'))
    expect(findButton(target, 'Send feedback')?.disabled).toBe(false)
  })

  it('sends the text, toasts a thank-you, and closes the dialog on success', async () => {
    const target = await mountDialog()
    feedbackFlow.open = true
    await typeFeedback(target, 'More keyboard shortcuts please!')

    findButton(target, 'Send feedback')?.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(sendFeedbackMock).toHaveBeenCalledWith('More keyboard shortcuts please!', undefined)
    expect(addToastMock).toHaveBeenCalled()
    expect(feedbackFlow.open).toBe(false)
  })

  it('shows a friendly retry message and keeps the dialog open on a soft failure', async () => {
    sendFeedbackMock.mockResolvedValue({ kind: 'softFailure' })
    const target = await mountDialog()
    feedbackFlow.open = true
    await typeFeedback(target, 'hello')

    findButton(target, 'Send feedback')?.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(feedbackFlow.open).toBe(true)
    expect(target.textContent).toContain('Try again?')
    // The user's text must survive the failed attempt.
    expect(target.querySelector('textarea')?.value).toBe('hello')
  })

  it('hides the attach-email checkbox when no beta email is on file', async () => {
    mockEmail = ''
    const target = await mountDialog()
    expect(target.textContent).not.toContain('Attach my email')
  })

  it('shows the attach-email checkbox, unticked, when an email is on file (sticky default off)', async () => {
    mockEmail = 'tester@example.com'
    mockAttachDefault = false
    const target = await mountDialog()
    const label = Array.from(target.querySelectorAll('label')).find((l) => l.textContent.includes('Attach my email'))
    const checkbox = label?.querySelector('input[type="checkbox"]')
    expect(checkbox).not.toBeNull()
    expect((checkbox as HTMLInputElement).checked).toBe(false)
    expect(target.textContent).toContain('tester@example.com')
  })

  it('includes the email in the send payload only when the box is checked, and persists the choice', async () => {
    mockEmail = 'tester@example.com'
    mockAttachDefault = false
    const target = await mountDialog()
    feedbackFlow.open = true
    await typeFeedback(target, 'hi')

    const label = Array.from(target.querySelectorAll('label')).find((l) => l.textContent.includes('Attach my email'))
    const checkbox = label?.querySelector('input[type="checkbox"]') as HTMLInputElement
    // The Checkbox primitive syncs state off the input's real click, not a manually
    // assigned `.checked` + dispatched change, so drive it the way a user would.
    checkbox.click()
    await tick()

    findButton(target, 'Send feedback')?.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(sendFeedbackMock).toHaveBeenCalledWith('hi', 'tester@example.com')
    expect(setSettingMock).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('routes the GitHub and book-a-call links through the opener instead of navigating', async () => {
    const target = await mountDialog()
    const links = Array.from(target.querySelectorAll('a'))
    expect(links.length).toBe(2)

    for (const link of links) {
      link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
    }
    await tick()

    expect(openExternalUrlMock).toHaveBeenCalledWith(GITHUB_ISSUES_URL)
    expect(openExternalUrlMock).toHaveBeenCalledWith(BOOK_A_CALL_URL)
  })

  it('Cancel button closes the dialog via the flow store', async () => {
    const target = await mountDialog()
    feedbackFlow.open = true
    findButton(target, 'Cancel')?.click()
    await tick()
    expect(feedbackFlow.open).toBe(false)
  })
})

describe('FeedbackDialog initial focus', () => {
  it('focuses the feedback textarea on open (keyboard-first)', async () => {
    feedbackFlow.open = true
    const target = await mountDialog()
    await tick()
    expect(document.activeElement).toBe(target.querySelector('#feedback-text'))
  })
})
