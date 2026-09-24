/**
 * The sent-crash-report toast's "Also send the log" action.
 *
 * The button IS the consent: crash reports are on by default and carry no log, while
 * `updates.errorReports` (an unbounded log bundle) is opt-in. So the tests that matter here are
 * about what happens without a press, and about the bundle being scoped to the CRASH rather than
 * to this launch.
 */

import { describe, expect, it, vi, beforeEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import CrashReportToastContent from './CrashReportToastContent.svelte'

// A tuple rest rather than two named params: `cmdr/no-confusable-callback-params` rightly objects to
// a callback type with two bare `string`s, and the assertions below check the pair as a whole.
const sendCrashLogReport = vi.hoisted(() => vi.fn<(...args: [string, string]) => Promise<{ id: string }>>())

vi.mock('$lib/ui/toast', () => ({ dismissToast: vi.fn() }))
vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/tauri-commands/error-reporter', () => ({ sendCrashLogReport }))

const sentReport = {
  version: 1,
  timestamp: '2026-03-22T10:00:00Z',
  signal: 'SIGSEGV',
  panicMessage: null,
  backtraceFrames: ['0x0000000104870988'],
  threadName: null,
  threadCount: 0,
  appVersion: '1.2.3',
  osVersion: 'macOS 26.7',
  arch: 'aarch64',
  uptimeSecs: 0,
  activeSettings: { indexingEnabled: true, aiProvider: 'off', mcpEnabled: false, verboseLogging: false },
  possibleCrashLoop: false,
  appFate: 'ended' as const,
  shortId: 'CRASH-V2SCH',
}

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CrashReportToastContent, { target, props: { report: sentReport } })
  flushSync()
  return target
}

function buttonWith(target: HTMLElement, text: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll('button')].find((b) => b.textContent.includes(text))
}

describe('CrashReportToastContent', () => {
  beforeEach(() => {
    sendCrashLogReport.mockReset()
    sendCrashLogReport.mockResolvedValue({ id: 'ERR-8RFN4' })
    document.body.innerHTML = ''
  })

  it('sends nothing until the button is pressed', () => {
    render()

    // The toast appears on its own after an auto-send. If merely rendering it shipped a log
    // bundle, the opt-in on `updates.errorReports` would be a lie.
    expect(sendCrashLogReport).not.toHaveBeenCalled()
  })

  it('scopes the bundle to the crash, and ties it to the crash report', async () => {
    const target = render()

    buttonWith(target, 'Attach logs')?.click()
    await tick()

    // The crash's own timestamp, never `Date.now()`: the lines worth reading are in the previous
    // session, and however long the machine sat closed is how wrong "the last hour" would be.
    expect(sendCrashLogReport).toHaveBeenCalledExactlyOnceWith('CRASH-V2SCH', '2026-03-22T10:00:00Z')
  })

  it('names the new report so the user can quote it, and drops the offer', async () => {
    const target = render()

    buttonWith(target, 'Attach logs')?.click()
    await vi.waitFor(() => {
      expect(target.textContent).toContain('ERR-8RFN4')
    })

    expect(buttonWith(target, 'Attach logs')).toBeUndefined()
  })

  it('drops the offer when the send does not land, rather than stacking a retry on a failure', async () => {
    // A toast carrying an invitation, a failure, and a retry all at once reads as three things
    // competing. The crash report itself already went out, so there's nothing urgent to recover.
    sendCrashLogReport.mockRejectedValueOnce(new Error('offline'))
    const target = render()

    buttonWith(target, 'Attach logs')?.click()
    await vi.waitFor(() => {
      expect(target.querySelector('[role="alert"]')).not.toBeNull()
    })

    expect(buttonWith(target, 'Attach logs')).toBeUndefined()
    expect(target.textContent).not.toContain('Would you like to help diagnose')
  })

  it('does not send twice when the button is pressed again mid-flight', async () => {
    let release: (value: { id: string }) => void = () => {}
    sendCrashLogReport.mockReturnValueOnce(new Promise((resolve) => (release = resolve)))
    const target = render()

    buttonWith(target, 'Attach logs')?.click()
    await tick()
    buttonWith(target, 'Sending…')?.click()
    await tick()

    expect(sendCrashLogReport).toHaveBeenCalledOnce()
    release({ id: 'ERR-8RFN4' })
  })
})
