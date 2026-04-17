/**
 * Tier 3 a11y tests for `CrashReportDialog.svelte`.
 *
 * Crash report modal with a JSON payload, "Always send" checkbox, and
 * send/dismiss actions.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CrashReportDialog from './CrashReportDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  sendCrashReport: vi.fn(() => Promise.resolve()),
  dismissCrashReport: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  setSetting: vi.fn(),
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

describe('CrashReportDialog a11y', () => {
  it('default (collapsed details) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CrashReportDialog, {
      target,
      props: { report: minimalReport, onClose: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
