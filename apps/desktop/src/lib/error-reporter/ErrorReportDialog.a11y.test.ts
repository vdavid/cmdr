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
      b.textContent?.includes("What's about to be sent"),
    )
    expect(toggle).toBeDefined()
    toggle?.click()
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
    const copyButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Copy')
    expect(copyButton).toBeDefined()
    copyButton?.click()
    await tick()
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('ERR-AB23X')
  })

  it('Cancel button closes the dialog via the flow store', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportDialog, { target, props: {} })
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    errorReportFlow.open = true
    const cancelButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Cancel')
    expect(cancelButton).toBeDefined()
    cancelButton?.click()
    await tick()
    expect(errorReportFlow.open).toBe(false)
  })
})
