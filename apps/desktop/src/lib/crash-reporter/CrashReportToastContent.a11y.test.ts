/**
 * Tier 3 a11y tests for `CrashReportToastContent.svelte`.
 *
 * Compact toast body shown after a report is sent. Just a text +
 * "Change in Settings > Updates" button. The report it's handed only picks
 * which sentence renders, so a single default-state test covers the markup.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CrashReportToastContent from './CrashReportToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

const sentReport = {
  version: 1,
  timestamp: '2026-03-22T10:00:00Z',
  signal: 'panic',
  panicMessage: 'called `unwrap()` on a `None` value',
  backtraceFrames: ['frame1'],
  threadName: 'tokio-runtime-worker',
  threadCount: 12,
  appVersion: '1.2.3',
  osVersion: 'macOS 15.3',
  arch: 'aarch64',
  uptimeSecs: 120,
  activeSettings: { indexingEnabled: true, aiProvider: 'off', mcpEnabled: false, verboseLogging: false },
  possibleCrashLoop: false,
  appFate: 'ended' as const,
}

describe('CrashReportToastContent a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CrashReportToastContent, { target, props: { report: sentReport } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
