/**
 * What `CrashReportDialog` does when the send doesn't land.
 *
 * It used to close exactly as it does on success, so a person believed a report went out that
 * never did. It stays open now, says so, and keeps Send as the retry. The log level follows the
 * typed failure: a network hiccup stays at warn, and a refusal from Cmdr's own server (a 4xx) is
 * the one that means Cmdr is broken, so it logs at error.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { ServerRequestError } from '$lib/ipc/bindings'
import { ServerRequestFailure } from '$lib/error-messages/server-request'
import CrashReportDialog from './CrashReportDialog.svelte'

const { sendCrashReport, dismissCrashReport, logger } = vi.hoisted(() => ({
  sendCrashReport: vi.fn<(report: unknown) => Promise<void>>(() => Promise.resolve()),
  dismissCrashReport: vi.fn(() => Promise.resolve()),
  logger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  sendCrashReport,
  dismissCrashReport,
}))

vi.mock('$lib/settings', () => ({
  setSetting: vi.fn(),
  getSetting: vi.fn(() => ''),
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => logger }))

const report = {
  version: 1,
  timestamp: '2026-09-13T10:00:00Z',
  signal: null,
  panicMessage: 'main thread panicked',
  backtraceFrames: ['frame1'],
  threadName: 'main',
  threadCount: 1,
  appVersion: '1.2.3',
  osVersion: 'macOS 15.3',
  arch: 'aarch64',
  uptimeSecs: 120,
  activeSettings: { indexingEnabled: true, aiProvider: 'off', mcpEnabled: false, verboseLogging: false },
  possibleCrashLoop: false,
}

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined
const onClose = vi.fn()

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(CrashReportDialog, { target, props: { report, onClose } })
  mounted = { target, instance }
  await tick()
  return target
}

function sendButton(target: HTMLElement): HTMLButtonElement {
  const button = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Send report')
  if (!button) throw new Error('no Send report button')
  return button
}

async function pressSendAndSettle(target: HTMLElement): Promise<void> {
  sendButton(target).click()
  // The rejection settles on a microtask, then the dialog re-renders.
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

describe('CrashReportDialog send', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(async () => {
    if (mounted) {
      await unmount(mounted.instance)
      mounted.target.remove()
      mounted = undefined
    }
  })

  it('closes once the report lands', async () => {
    const target = await mountDialog()
    await pressSendAndSettle(target)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('stays open when the server can’t be reached, says so, and keeps Send as the retry', async () => {
    const offline: ServerRequestError = { type: 'unreachable', detail: 'error sending request for url' }
    sendCrashReport.mockRejectedValueOnce(new ServerRequestFailure(offline))
    const target = await mountDialog()

    await pressSendAndSettle(target)

    expect(onClose).not.toHaveBeenCalled()
    const alert = target.querySelector('[role="alert"]')
    expect(alert?.textContent).toContain('The report didn’t go out.')
    expect(alert?.textContent).toContain('Check your internet connection')
    expect(alert?.textContent).not.toContain('error sending request')
    expect(sendButton(target).disabled).toBe(false)
    expect(logger.warn).toHaveBeenCalledTimes(1)
    expect(logger.error).not.toHaveBeenCalled()
  })

  it('logs a refusal from Cmdr’s own server at error, since that means Cmdr and its server disagree', async () => {
    sendCrashReport.mockRejectedValueOnce(new ServerRequestFailure({ type: 'refused', status: 400, detail: '{}' }))
    const target = await mountDialog()

    await pressSendAndSettle(target)

    expect(onClose).not.toHaveBeenCalled()
    expect(logger.error).toHaveBeenCalledTimes(1)
    expect(logger.warn).not.toHaveBeenCalled()
  })
})
