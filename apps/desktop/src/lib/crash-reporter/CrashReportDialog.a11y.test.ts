/**
 * Tier 3 a11y tests for `CrashReportDialog.svelte`.
 *
 * Crash report modal with a JSON payload, "Always send" checkbox, and
 * send/dismiss actions.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CrashReportDialog from './CrashReportDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  sendCrashReport: vi.fn(() => Promise.resolve()),
  dismissCrashReport: vi.fn(() => Promise.resolve()),
}))

/** Live listeners on `analytics.email`, so a test can play the Settings window's part. */
const emailListeners = new Set<(value: string) => void>()
let mockEmail = ''

vi.mock('$lib/settings', () => ({
  setSetting: vi.fn(),
  getSetting: vi.fn((id: string) => (id === 'analytics.email' ? mockEmail : false)),
  onSpecificSettingChange: (id: string, listener: (value: string) => void) => {
    if (id !== 'analytics.email') return () => {}
    emailListeners.add(listener)
    return () => emailListeners.delete(listener)
  },
}))

const minimalReport = {
  version: 1,
  timestamp: '2025-04-16T10:00:00Z',
  signal: null,
  panicMessage: 'main thread panicked',
  backtraceFrames: ['frame1', 'frame2'],
  threadName: 'main',
  threadCount: 1,
  appVersion: '1.2.3',
  osVersion: 'macOS 15.3',
  arch: 'aarch64',
  uptimeSecs: 120,
  activeSettings: {
    indexingEnabled: true,
    aiProvider: 'off',
    mcpEnabled: false,
    verboseLogging: false,
  },
  possibleCrashLoop: false,
}

/** Mount the dialog and settle, so Ark's checkbox machine is running before any click. */
async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CrashReportDialog, {
    target,
    props: { report: minimalReport, onClose: () => {} },
  })
  await tick()
  return target
}

/** The user edits their contact email in the Settings window while the dialog stays up. */
async function setContactEmailFromSettings(value: string): Promise<void> {
  mockEmail = value
  for (const listener of [...emailListeners]) listener(value)
  await tick()
}

function attachEmailCheckbox(target: HTMLElement): HTMLInputElement | null {
  const label = Array.from(target.querySelectorAll('label')).find((l) => l.textContent.includes('Attach my email'))
  return label?.querySelector('input[type="checkbox"]') ?? null
}

describe('CrashReportDialog a11y', () => {
  it('default (collapsed details) has no a11y violations', async () => {
    mockEmail = ''
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })

  it('asks for an address when the one it was naming is cleared in Settings', async () => {
    mockEmail = 'tester@example.com'
    const target = await mountDialog()
    attachEmailCheckbox(target)?.click()
    await tick()
    expect(target.querySelector('input[type="email"]')).toBeNull()

    await setContactEmailFromSettings('')

    // The tick stands, but it can no longer mean an address the user can't see: the field
    // comes back empty and the report goes out without one until they fill it in.
    expect(attachEmailCheckbox(target)?.checked).toBe(true)
    expect(target.querySelector<HTMLInputElement>('input[type="email"]')?.value).toBe('')
    await expectNoA11yViolations(target)
  })
})
