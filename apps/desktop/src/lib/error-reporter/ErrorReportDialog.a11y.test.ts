/**
 * Tier 3 a11y tests for `ErrorReportDialog.svelte`.
 *
 * The dialog exposes a textarea, a manifest preview, and send/cancel actions.
 * The `prepareErrorReportPreview` IPC is mocked so the test runs deterministically.
 */

import { describe, it, vi, expect, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import ErrorReportDialog from './ErrorReportDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { closeErrorReportDialog, errorReportFlow } from './error-report-flow.svelte'

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
    // Tests run in Vitest's `test` mode where `import.meta.env.DEV` is true
    // by default. Pretend we're a release build so the dialog's dev-mode
    // Send-disable doesn't shadow the assertions below — there's a separate
    // test ("disables Send in dev builds") for the dev-disable behavior.
    vi.stubEnv('DEV', false)
  })

  afterEach(() => {
    vi.unstubAllEnvs()
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
    const oneEmoji = '\u{1F680}' // rocket — 1 code point, 2 UTF-16 units
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

    // Send button must still be enabled — 50 001 < 100 000 (the hard cap).
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
    // 100 001 code-points of emoji — 200 002 UTF-16 units. Naive `.length` would have
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
