/**
 * Tier 3 a11y tests for the error reporter: the report dialog and the three
 * toasts it can leave behind.
 *
 * One file per component would cost about four times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * Two stubs needed care. `$lib/settings` is stubbed only for the dialog (its
 * `getSetting` answers every non-email key with the attach-default flag, which
 * would quietly answer for the toasts too), so it's a mutable the dialog's block
 * installs in its own `beforeEach`, with `null` meaning "use the real export".
 * `./error-report-flow.svelte` is stubbed by the auto-send block but used FOR REAL
 * by the dialog's, which drives `errorReportFlow.open` and asserts on it; spreading
 * the real module and overriding only `openErrorReportDialog` gives each block
 * exactly what it had. The amend entry point is deliberately NOT stubbed: what the
 * auto-sent toast must do is land the store in amend mode, and asserting on the real
 * store is what proves the toast can't reach the compose path.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createRawSnippet, mount, tick, type Component } from 'svelte'
import AutoSendToastContent from './AutoSendToastContent.svelte'
import BundleSavedToastContent from './BundleSavedToastContent.svelte'
import ErrorReportDialog from './ErrorReportDialog.svelte'
import ErrorReportToastContent from './ErrorReportToastContent.svelte'
import SentReportToastBody from './SentReportToastBody.svelte'
import { setLastAutoSentReportId, getLastAutoSentReportId } from './auto-send-toast-state.svelte'
import { setLastSavedBundlePath } from './bundle-saved-toast-state.svelte'
import { setLastSentReport, getLastSentReportId } from './error-report-toast-state.svelte'
import { closeErrorReportDialog, errorReportFlow, openErrorReportDialog } from './error-report-flow.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { dismissToast } from '$lib/ui/toast'
import { showInFinder } from '$lib/tauri-commands'
import { amendErrorReport, prepareErrorReportPreview, sendErrorReport } from '$lib/tauri-commands/error-reporter'
import { openSettingsWindow } from '$lib/settings/settings-window'

// What `getSetting` answers. `null` means "use the real export", which is what the
// three toast blocks, which never stubbed settings, always saw.
let settingsStub: ((id: string) => unknown) | null = null
const setSettingMock = vi.fn()

vi.mock('$lib/ui/toast', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dismissToast: vi.fn(),
  addToast: vi.fn(),
}))

vi.mock('$lib/settings/settings-window', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

vi.mock('./error-report-flow.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openErrorReportDialog: vi.fn(),
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  showInFinder: vi.fn(() => Promise.resolve()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

/** Live listeners on `analytics.email`, so a test can play the Settings window's part. */
const emailListeners = new Set<(value: string) => void>()

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<{ getSetting: (id: string) => unknown }>()
  return {
    ...actual,
    getSetting: vi.fn((id: string): unknown => (settingsStub ? settingsStub(id) : actual.getSetting(id))),
    setSetting: (id: string, value: unknown) => {
      setSettingMock(id, value)
    },
    onSpecificSettingChange: (id: string, listener: (value: string) => void) => {
      if (id !== 'analytics.email') return () => {}
      emailListeners.add(listener)
      return () => emailListeners.delete(listener)
    },
  }
})

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
    system: {
      osBuild: '25F80',
      macModel: 'Mac15,9',
      cpuPhysical: 12,
      cpuLogical: 12,
      preferredLanguage: 'en-US',
      totalMemoryBytes: 68_719_476_736,
      dataVolumeFreeBytes: 100_000_000_000,
      dataVolumeTotalBytes: 500_000_000_000,
      indexTotalBytes: 1_624,
      indexDbSizes: [1_524, 100],
      live: null,
    },
    generatedAt: '2026-04-23T10:00:00+00:00',
  },
  sampleFirst: ['INFO line 1', 'INFO line 2'],
  sampleLast: ['DEBUG last line'],
  totalRedactedLines: 42,
}

// What Flow B auto-sent: a DIFFERENT id from the compose preview's, so a test that
// passes can only be reading the stash, never a freshly built bundle.
const autoSentPayload = {
  ...previewPayload,
  id: 'ERR-AUTO9',
  canAmend: true,
  manifest: { ...previewPayload.manifest, id: 'ERR-AUTO9', kind: 'auto' as const },
}

// Per-test stash state. Read inside the mock's closures, so the hoisted factory is fine.
let autoSentStash: typeof autoSentPayload | null = autoSentPayload
let autoSentCanAmend = true
let autoSentThrows = false

vi.mock('$lib/tauri-commands/error-reporter', () => ({
  prepareErrorReportPreview: vi.fn(() => Promise.resolve(previewPayload)),
  sendErrorReport: vi.fn(() => Promise.resolve({ id: 'ERR-AB23X' })),
  amendErrorReport: vi.fn(() => Promise.resolve({ id: 'ERR-AUTO9' })),
  getAutoSentReportPreview: vi.fn(() =>
    autoSentThrows
      ? Promise.reject(new Error('the stash is gone'))
      : Promise.resolve(autoSentStash && { ...autoSentStash, canAmend: autoSentCanAmend }),
  ),
  saveErrorReportToDisk: vi.fn(() => Promise.resolve('/tmp/bundle.zip')),
}))

// jsdom doesn't ship navigator.clipboard; stub it for the copy tests.
Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn(() => Promise.resolve()) },
  writable: true,
})

/** Mounts a props-less component into a fresh container attached to the document. */
function mountInto(component: Component<Record<string, never>>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props: {} })
  return target
}

// These components share one jsdom document, the dialog portals into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `AutoSendToastContent.svelte`.
 *
 * Toast body shown after the Flow B auto-dispatcher uploads a report. Reads the last
 * auto-sent ID from a module-level `$state` set via `setLastAutoSentReportId(id)`.
 */
describe('AutoSendToastContent', () => {
  it('default render has no a11y violations', async () => {
    setLastAutoSentReportId('ERR-AUTO1')
    const target = mountInto(AutoSendToastContent)
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the most recently set auto-sent ID', () => {
    setLastAutoSentReportId('ERR-AUTO2')
    expect(getLastAutoSentReportId()).toBe('ERR-AUTO2')
    const target = mountInto(AutoSendToastContent)
    expect(target.textContent).toContain('ERR-AUTO2')
    expect(target.textContent).toContain('Error report sent')
    expect(target.textContent).toContain('Reference ID')
  })

  // The incident this guards: the button used to call the compose entry point, so a
  // note typed after an auto-send uploaded a SECOND report under a third id.
  it('the view/add-notes button opens the dialog in amend mode, never the compose one', async () => {
    setLastAutoSentReportId('ERR-VIEW1')
    closeErrorReportDialog()
    const target = mountInto(AutoSendToastContent)
    await tick()
    const viewButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'View or add notes to the report',
    )
    if (!viewButton) throw new Error('View or add notes button missing')
    viewButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-auto-sent')
    expect(openErrorReportDialog).not.toHaveBeenCalled()
    expect(errorReportFlow.open).toBe(true)
    expect(errorReportFlow.mode).toBe('amend')
  })

  it('Change settings button dismisses the toast and opens the settings window', async () => {
    setLastAutoSentReportId('ERR-SET01')
    const target = mountInto(AutoSendToastContent)
    await tick()
    const settingsButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Change settings',
    )
    if (!settingsButton) throw new Error('Change settings button missing')
    settingsButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-auto-sent')
    expect(openSettingsWindow).toHaveBeenCalled()
  })
})

/**
 * Tier 3 a11y tests for `BundleSavedToastContent.svelte`.
 *
 * Toast body shown after a successful "Save bundle to disk (debug)" action.
 * Reads the saved-bundle path from a module-level `$state` set via
 * `setLastSavedBundlePath(path)`.
 */
describe('BundleSavedToastContent', () => {
  it('default render has no a11y violations', async () => {
    setLastSavedBundlePath('/Users/test/Application Support/com.veszelovszki.cmdr-dev/error-report-debug.zip')
    const target = mountInto(BundleSavedToastContent)
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the most recently saved path', () => {
    setLastSavedBundlePath('/tmp/bundle-XYZ.zip')
    const target = mountInto(BundleSavedToastContent)
    expect(target.textContent).toContain('/tmp/bundle-XYZ.zip')
  })

  it('Reveal in Finder button calls showInFinder with the saved path', async () => {
    setLastSavedBundlePath('/tmp/bundle-REV.zip')
    const target = mountInto(BundleSavedToastContent)
    await tick()
    const revealButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Reveal in Finder',
    )
    if (!revealButton) throw new Error('Reveal in Finder button missing')
    revealButton.click()
    expect(showInFinder).toHaveBeenCalledWith('/tmp/bundle-REV.zip')
  })

  it('Dismiss button calls dismissToast with the toast ID', async () => {
    setLastSavedBundlePath('/tmp/bundle-DIS.zip')
    const target = mountInto(BundleSavedToastContent)
    await tick()
    const dismissButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Dismiss')
    if (!dismissButton) throw new Error('Dismiss button missing')
    dismissButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-bundle-saved')
  })
})

/**
 * Tier 3 a11y tests for `ErrorReportToastContent.svelte`.
 *
 * Toast body shown after a successful error-report send. Reads the last sent ID
 * from a module-level `$state` set via `setLastSentReport({ id, kind })`.
 */
describe('ErrorReportToastContent', () => {
  it('default render has no a11y violations', async () => {
    setLastSentReport({ id: 'ERR-AB23X', kind: 'sent' })
    const target = mountInto(ErrorReportToastContent)
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the most recently set sent ID', () => {
    setLastSentReport({ id: 'ERR-99XYZ', kind: 'sent' })
    expect(getLastSentReportId()).toBe('ERR-99XYZ')
    const target = mountInto(ErrorReportToastContent)
    expect(target.textContent).toContain('ERR-99XYZ')
  })

  it('Copy ID button copies to the clipboard', async () => {
    setLastSentReport({ id: 'ERR-COPY1', kind: 'sent' })
    const target = mountInto(ErrorReportToastContent)
    await tick()
    const copyButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Copy ID')
    if (!copyButton) throw new Error('Copy ID button missing')
    copyButton.click()
    await tick()
    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest spy on prototype method
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('ERR-COPY1')
  })

  it('Dismiss button calls dismissToast with the toast ID', async () => {
    setLastSentReport({ id: 'ERR-DISMS', kind: 'sent' })
    const target = mountInto(ErrorReportToastContent)
    await tick()
    const dismissButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Dismiss')
    if (!dismissButton) throw new Error('Dismiss button missing')
    dismissButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-sent')
  })
})

/**
 * Tier 3 a11y tests for `SentReportToastBody.svelte`, the body shared by the two
 * "a report went out" toasts. The two shapes it can take are the ones the callers
 * pick between: with a bold lead line, and without one, where the sentence carries
 * the news itself and the id badge has to stay part of that sentence.
 */
describe('SentReportToastBody', () => {
  const actions = createRawSnippet(() => ({
    render: () => '<button type="button">Dismiss</button>',
  }))

  function mountBody(title?: string): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SentReportToastBody, {
      target,
      props: { title, message: 'Reference ID:', reportId: 'ERR-AB23X', actions },
    })
    return target
  }

  it('with a title has no a11y violations', async () => {
    const target = mountBody('Error report sent')
    await tick()
    expect(target.textContent).toContain('ERR-AB23X')
    await expectNoA11yViolations(target)
  })

  it('without a title has no a11y violations', async () => {
    const target = mountBody()
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ErrorReportDialog.svelte`.
 *
 * The dialog exposes a textarea, a manifest preview, and send/cancel actions.
 * The `prepareErrorReportPreview` IPC is mocked so the test runs deterministically.
 */
describe('ErrorReportDialog', () => {
  // Settings are mocked per-test via these refs so the email-on-file and sticky-default
  // states can vary. Defaults: no email on file, attach-default off.
  let mockEmail = ''
  let mockAttachDefault = false

  beforeEach(() => {
    closeErrorReportDialog()
    mockEmail = ''
    mockAttachDefault = false
    settingsStub = (id: string): unknown => (id === 'analytics.email' ? mockEmail : mockAttachDefault)
    setSettingMock.mockClear()
    vi.mocked(sendErrorReport).mockClear()
    vi.mocked(prepareErrorReportPreview).mockClear()
  })

  afterEach(() => {
    settingsStub = null
  })

  it('default render has no a11y violations', async () => {
    await expectNoA11yViolations(await mountSettled())
  })

  it('renders the preview ID once the preview resolves', async () => {
    const target = await mountSettled()
    expect(target.textContent).toContain('ERR-AB23X')
  })

  it('expanding "What\'s about to be sent" reveals the manifest', async () => {
    const target = await mountSettled()
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
    const target = await mountSettled()
    const textarea = target.querySelector('textarea')
    expect(textarea).toBeDefined()
    if (!textarea) throw new Error('textarea missing')
    textarea.value = 'something broke'
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(textarea.value).toBe('something broke')
  })

  it('Copy button copies the preview ID to the clipboard', async () => {
    const target = await mountSettled()
    const copyButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Copy')
    if (!copyButton) throw new Error('Copy button missing')
    copyButton.click()
    await tick()
    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest spy on prototype method
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('ERR-AB23X')
  })

  it('counts emoji-heavy notes by code point so the cap matches the backend', async () => {
    const target = await mountSettled()

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
    const target = await mountSettled()

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
    const target = mountInto(ErrorReportDialog)
    await tick()
    // Wait for the debounced preview load to settle.
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    return target
  }

  it('still offers the attach-email checkbox when no beta email is on file', async () => {
    mockEmail = ''
    const target = await mountSettled()
    expect(findAttachEmailCheckbox(target)?.checked).toBe(false)
    expect(target.textContent).toContain('Attach my email address so you can follow up')
    // The field is the tick's reward, so an untouched dialog asks nothing extra.
    expect(target.querySelector('input[type="email"]')).toBeNull()
  })

  it('reveals a field for a reply address, and holds the send back until it can be one', async () => {
    mockEmail = ''
    const target = await mountSettled()
    findAttachEmailCheckbox(target)?.click()
    await tick()

    const input = target.querySelector<HTMLInputElement>('input[type="email"]')
    if (!input) throw new Error('email input missing')
    input.value = 'tester'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    expect(sendButton?.disabled).toBe(true)
    expect(target.textContent).toContain("doesn't look like an email address")
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

  it('carries the address the user corrected in Settings while the dialog stayed up', async () => {
    mockEmail = 'old@example.com'
    mockAttachDefault = false
    const target = await mountSettled()
    findAttachEmailCheckbox(target)?.click()
    await tick()

    // Settings opens as its own window, so the dialog is still here when this lands.
    mockEmail = 'new@example.com'
    for (const listener of [...emailListeners]) listener('new@example.com')
    await tick()

    expect(target.textContent).toContain('new@example.com')
    expect(target.textContent).not.toContain('old@example.com')

    errorReportFlow.open = true
    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    if (!sendButton) throw new Error('Send button missing')
    sendButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(vi.mocked(sendErrorReport)).toHaveBeenCalledWith(undefined, 'new@example.com', 'ERR-AB23X')
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
    // The Checkbox primitive syncs state off the input's real click, not a manually
    // assigned `.checked` + dispatched change, so drive it the way a user would.
    checkbox.click()
    await tick()

    errorReportFlow.open = true
    const sendButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.trim().startsWith('Send report'),
    )
    if (!sendButton) throw new Error('Send button missing')
    sendButton.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(vi.mocked(sendErrorReport)).toHaveBeenCalledWith(undefined, 'tester@example.com', 'ERR-AB23X')
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

    expect(vi.mocked(sendErrorReport)).toHaveBeenCalledWith(undefined, undefined, 'ERR-AB23X')
  })

  it('Cancel button closes the dialog via the flow store', async () => {
    const target = await mountSettled()
    errorReportFlow.open = true
    const cancelButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Cancel')
    if (!cancelButton) throw new Error('Cancel button missing')
    cancelButton.click()
    await tick()
    expect(errorReportFlow.open).toBe(false)
  })

  it('focuses the note textarea on open (keyboard-first)', async () => {
    const target = await mountSettled()
    await tick()
    expect(document.activeElement).toBe(target.querySelector('#error-report-note'))
  })

  // The preview build used to take the live email as an argument, which made it a tracked
  // dependency: every keystroke in the field re-ran a multi-MB bundle build and minted a
  // fresh report id under the cursor. One call, whatever the user touches.
  it('builds the bundle once, however much the email opt-in is fiddled with', async () => {
    mockEmail = ''
    const target = await mountSettled()
    expect(vi.mocked(prepareErrorReportPreview)).toHaveBeenCalledTimes(1)

    const checkbox = findAttachEmailCheckbox(target)
    checkbox?.click()
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="email"]')
    if (!input) throw new Error('email input missing')
    for (const value of ['t', 'te', 'tester@example.com']) {
      input.value = value
      input.dispatchEvent(new Event('input', { bubbles: true }))
      await tick()
    }
    checkbox?.click()
    await tick()

    expect(vi.mocked(prepareErrorReportPreview)).toHaveBeenCalledTimes(1)
    // And the manifest still shows the live choice, which is what made the rebuild pointless.
    expect(vi.mocked(prepareErrorReportPreview)).toHaveBeenCalledWith()
  })
})

/**
 * Amend mode: the dialog the Flow B auto-sent toast opens.
 *
 * Its whole job is that ONE incident stays ONE report, so every test here is really
 * asking the same question two ways: is the id on screen the id that shipped, and is
 * `sendErrorReport` still untouched?
 */
describe('ErrorReportDialog in amend mode', () => {
  beforeEach(() => {
    closeErrorReportDialog()
    settingsStub = (id: string): unknown => (id === 'analytics.email' ? '' : false)
    autoSentStash = autoSentPayload
    autoSentCanAmend = true
    autoSentThrows = false
    setSettingMock.mockClear()
    vi.mocked(sendErrorReport).mockClear()
    vi.mocked(amendErrorReport).mockClear()
  })

  afterEach(() => {
    settingsStub = null
  })

  /** Seeds the store the way `openErrorReportDialogForAutoSentReport` does, then mounts. */
  async function mountAmend(): Promise<HTMLElement> {
    errorReportFlow.mode = 'amend'
    errorReportFlow.open = true
    const target = mountInto(ErrorReportDialog)
    await tick()
    await new Promise((r) => setTimeout(r, 300))
    await tick()
    return target
  }

  function findButton(target: HTMLElement, label: string): HTMLButtonElement | undefined {
    return Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === label)
  }

  async function typeNote(target: HTMLElement, note: string): Promise<void> {
    const textarea = target.querySelector('textarea')
    if (!textarea) throw new Error('textarea missing')
    textarea.value = note
    textarea.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
  }

  it('default render has no a11y violations', async () => {
    const target = await mountAmend()
    await expectNoA11yViolations(target)
  })

  it('shows the id that actually shipped, and copies that one', async () => {
    const target = await mountAmend()
    expect(target.textContent).toContain('ERR-AUTO9')
    expect(target.textContent).not.toContain('ERR-AB23X')
    findButton(target, 'Copy')?.click()
    await tick()
    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest spy on prototype method
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('ERR-AUTO9')
  })

  it('adds the note to the report already sent, and uploads nothing new', async () => {
    const target = await mountAmend()
    await typeNote(target, 'it happened while copying')
    const submit = findButton(target, 'Add to report')
    if (!submit) throw new Error('Add to report button missing')
    submit.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(vi.mocked(amendErrorReport)).toHaveBeenCalledWith('it happened while copying', undefined)
    expect(vi.mocked(sendErrorReport)).not.toHaveBeenCalled()
    expect(getLastSentReportId()).toBe('ERR-AUTO9')
    expect(errorReportFlow.open).toBe(false)
  })

  it('holds the button back until there is a note or an email to carry', async () => {
    const target = await mountAmend()
    expect(findButton(target, 'Add to report')?.disabled).toBe(true)
    await typeNote(target, 'here you go')
    expect(findButton(target, 'Add to report')?.disabled).toBe(false)
  })

  it('offers no submit at all when nothing was auto-sent this run', async () => {
    autoSentStash = null
    const target = await mountAmend()
    expect(target.textContent).toContain("That report can't take a note any more")
    expect(findButton(target, 'Add to report')).toBeUndefined()
    expect(target.querySelector('textarea')).toBeNull()
    expect(vi.mocked(sendErrorReport)).not.toHaveBeenCalled()
  })

  it('offers no submit at all when the report can no longer be added to', async () => {
    autoSentCanAmend = false
    const target = await mountAmend()
    expect(target.textContent).toContain("That report can't take a note any more")
    expect(findButton(target, 'Add to report')).toBeUndefined()
    expect(vi.mocked(sendErrorReport)).not.toHaveBeenCalled()
  })

  it('lands on the same dead end when the stash lookup throws', async () => {
    autoSentThrows = true
    const target = await mountAmend()
    expect(target.textContent).toContain("That report can't take a note any more")
    expect(vi.mocked(sendErrorReport)).not.toHaveBeenCalled()
  })

  it('drops the debug save-to-disk button: there is no local bundle to save', async () => {
    const target = await mountAmend()
    expect(findButton(target, 'Save bundle to disk (debug)')).toBeUndefined()
  })

  it('carries a reply address the person types right here', async () => {
    const target = await mountAmend()
    const label = Array.from(target.querySelectorAll('label')).find((l) => l.textContent.includes('Attach my email'))
    label?.querySelector<HTMLInputElement>('input[type="checkbox"]')?.click()
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="email"]')
    if (!input) throw new Error('email input missing')
    input.value = 'tester@example.com'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    // An address alone is enough: the note stays empty here on purpose.
    const submit = findButton(target, 'Add to report')
    expect(submit?.disabled).toBe(false)
    submit?.click()
    await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(vi.mocked(amendErrorReport)).toHaveBeenCalledWith(undefined, 'tester@example.com')
    // Persisted only after the amend resolved, same as every other dialog.
    expect(setSettingMock).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })
})
