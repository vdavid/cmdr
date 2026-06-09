/**
 * Tier 3 a11y tests for `ErrorReportDialog.svelte`.
 *
 * The dialog exposes a textarea, a manifest preview, and send/cancel actions.
 * The `prepareErrorReportPreview` IPC is mocked so the test runs deterministically.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import ErrorReportDialog from './ErrorReportDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { closeErrorReportDialog, errorReportFlow } from './error-report-flow.svelte'
import { sendErrorReport } from '$lib/tauri-commands/error-reporter'

const previewPayload = {
  id: 'ERR-AB23X',
  sizeBytes: 12345,
  manifest: {
    id: 'ERR-AB23X',
    kind: 'user' as const,
    appVersion: '0.0.0-test',
    osVersion: 'macOS test',
    arch: 'aarch64',
    activeSettings: {
      indexingEnabled: true,
      aiProvider: 'off',
      mcpEnabled: false,
      verboseLogging: false,
    },
    generatedAt: '2026-04-23T10:00:00+00:00',
  },
  sampleFirst: ['INFO line 1', 'INFO line 2'],
  sampleLast: ['DEBUG last line'],
  totalRedactedLines: 42,
}

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands/error-reporter', () => ({
  prepareErrorReportPreview: vi.fn(() => Promise.resolve(previewPayload)),
  sendErrorReport: vi.fn(() => Promise.resolve({ id: 'ERR-AB23X' })),
  saveErrorReportToDisk: vi.fn(() => Promise.resolve('/tmp/bundle.zip')),
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

vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
}))

// jsdom doesn't ship navigator.clipboard.
Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn(() => Promise.resolve()) },
  writable: true,
})

describe('ErrorReportDialog', () => {
  beforeEach(() => {
    closeErrorReportDialog()
    mockEmail = ''
    mockAttachDefault = false
    setSettingMock.mockClear()
    vi.mocked(sendErrorReport).mockClear()
  })

  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    // Wait for the debounced preview load to settle.
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the preview ID once the preview resolves', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    expect(target.textContent).toContain('ERR-AB23X')
  })

  it('expanding "What\'s about to be sent" reveals the manifest', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    const toggle = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.includes("What's about to be sent"),
    )
    if (!toggle) throw new Error('toggle missing')
    toggle.click()
    await tick()
    expect(target.textContent).toContain('Manifest')
    expect(target.textContent).toContain('Sample of first')
    expect(target.textContent).toContain('Sample of last')
  })

  it('typing in the textarea updates the note', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    const textarea = target.querySelector('textarea')
    expect(textarea).toBeDefined()
    if (!textarea) throw new Error('textarea missing')
    textarea.value = 'something broke'
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(textarea.value).toBe('something broke')
  })

  it('Copy button copies the preview ID to the clipboard', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    const copyButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Copy')
    if (!copyButton) throw new Error('Copy button missing')
    copyButton.click()
    await tick()
    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest spy on prototype method
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('ERR-AB23X')
  })

  it('counts emoji-heavy notes by code point so the cap matches the backend', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()

    // Each rocket emoji is two UTF-16 code units but one Unicode code point. With ~50k
    // emoji we sit in the soft-warning band by code-point count; the displayed counter
    // must show the code-point count, not the doubled UTF-16 length.
    const textarea = target.querySelector('textarea')
    expect(textarea).not.toBeNull()
    if (!textarea) throw new Error('textarea missing')
    const oneEmoji = '\u{1F680}' // rocket: 1 code point, 2 UTF-16 units
    // Use 50 001 emoji so we exceed the soft-warn threshold (50 000) by code points.
    const note = oneEmoji.repeat(50_001)
    textarea.value = note
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    // The counter only renders past the soft-warn threshold, so its appearance plus the
    // formatted code-point count proves both the count is correct and the threshold
    // gating uses the same scheme.
    const counter = target.querySelector('.note-counter')
    expect(counter).not.toBeNull()
    expect(counter?.textContent).toContain((50_001).toLocaleString('en-US'))

    // Send button must still be enabled: 50 001 < 100 000 (the hard cap).
    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    expect(sendButton).toBeDefined()
    expect(sendButton?.disabled).toBe(false)
  })

  it('disables Send when a code-point count exceeds the hard cap', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()

    const textarea = target.querySelector('textarea')
    if (!textarea) throw new Error('textarea missing')
    // 100 001 code-points of emoji (200 002 UTF-16 units). Naive `.length` would have
    // already reported "over" at 50 001 emoji; what we're checking here is that the
    // boundary at 100 000 also fires, regardless of representation.
    textarea.value = '\u{1F680}'.repeat(100_001)
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    expect(sendButton?.disabled).toBe(true)
  })

  function findAttachEmailCheckbox(target: HTMLElement): HTMLInputElement | null {
    const label = Array.from(target.querySelectorAll('label')).find((l) => l.textContent.includes('Attach my email'))
    return label?.querySelector('input[type="checkbox"]') ?? null
  }

  async function mountSettled(): Promise<HTMLElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    return target
  }

  it('hides the attach-email checkbox when no beta email is on file', async () => {
    mockEmail = ''
    const target = await mountSettled()
    expect(findAttachEmailCheckbox(target)).toBeNull()
  })

  it('shows the attach-email checkbox, unticked, when an email is on file (sticky default off)', async () => {
    mockEmail = 'tester@example.com'
    mockAttachDefault = false
    const target = await mountSettled()
    const checkbox = findAttachEmailCheckbox(target)
    expect(checkbox).not.toBeNull()
    expect(checkbox?.checked).toBe(false)
    expect(target.textContent).toContain('tester@example.com')
  })

  it('pre-ticks the checkbox when the sticky default is on', async () => {
    mockEmail = 'tester@example.com'
    mockAttachDefault = true
    const target = await mountSettled()
    expect(findAttachEmailCheckbox(target)?.checked).toBe(true)
  })

  it('includes the email in the send payload only when the box is checked, and persists the choice', async () => {
    mockEmail = 'tester@example.com'
    mockAttachDefault = false
    const target = await mountSettled()
    const checkbox = findAttachEmailCheckbox(target)
    if (!checkbox) throw new Error('checkbox missing')
    checkbox.checked = true
    checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()

    errorReportFlow.open = true
    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    if (!sendButton) throw new Error('Send button missing')
    sendButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(vi.mocked(sendErrorReport)).toHaveBeenCalledWith(undefined, 'tester@example.com')
    expect(setSettingMock).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('omits the email from the send payload when the box is unchecked', async () => {
    mockEmail = 'tester@example.com'
    mockAttachDefault = false
    const target = await mountSettled()

    errorReportFlow.open = true
    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    if (!sendButton) throw new Error('Send button missing')
    sendButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(vi.mocked(sendErrorReport)).toHaveBeenCalledWith(undefined, undefined)
  })

  it('Cancel button closes the dialog via the flow store', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    errorReportFlow.open = true
    const cancelButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Cancel')
    if (!cancelButton) throw new Error('Cancel button missing')
    cancelButton.click()
    await tick()
    expect(errorReportFlow.open).toBe(false)
  })
})
