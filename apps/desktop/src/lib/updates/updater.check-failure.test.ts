/**
 * What a failed update check keeps and logs, on the macOS path where the check is typed.
 *
 * A manual check used to store the raw backend string ("Couldn't fetch update manifest: error sending request for
 * url (…)"), which the toast and Settings printed after "Error:", and an offline laptop logged one warn per hourly
 * tick. The failure is typed now: network trouble logs once until a check lands, and only a manifest Cmdr's own
 * server refused or served unreadable logs at error, since that means the contract broke.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ServerRequestError } from '$lib/ipc/bindings'
import { ServerRequestFailure } from '$lib/error-messages/server-request'
import { UpdateDownloadFailure } from './update-download-failure'

const { checkForUpdateMock, downloadUpdateMock, installUpdateMock, logger } = vi.hoisted(() => ({
  checkForUpdateMock: vi.fn(),
  downloadUpdateMock: vi.fn(() => Promise.resolve()),
  installUpdateMock: vi.fn(() => Promise.resolve()),
  logger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({
  checkForUpdate: checkForUpdateMock,
  downloadUpdate: downloadUpdateMock,
  installUpdate: installUpdateMock,
  updateWriteBlocker: vi.fn(() => Promise.resolve(null)),
  updateCheckDueIn: vi.fn(() => Promise.resolve(0)),
  recordUpdateCheck: vi.fn(() => Promise.resolve()),
  trackEvent: vi.fn(),
}))

// The typed check lives on the macOS branch.
vi.mock('$lib/shortcuts/key-capture', () => ({ isMacOS: () => true }))

vi.mock('$lib/ui/toast', () => ({ addToast: vi.fn(), dismissToast: vi.fn() }))

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn((id: string) => (id === 'onboarding.completed' ? true : 60 * 60 * 1000)),
  setSetting: vi.fn(),
  forceSave: vi.fn(() => Promise.resolve(true)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(() => Promise.resolve('0.28.0')) }))

vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => logger }))

import { _resetUpdaterStateForTest, checkForUpdates, updateState } from './updater.svelte'

const offline: ServerRequestError = {
  type: 'unreachable',
  detail: 'error sending request for url (https://api.getcmdr.com/update-check/0.28.0?arch=aarch64)',
}

describe('a failed update check', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    vi.clearAllMocks()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('keeps the typed failure on the state, never the raw text', async () => {
    checkForUpdateMock.mockRejectedValueOnce(new ServerRequestFailure(offline))

    await checkForUpdates('command')

    expect(updateState.status).toBe('idle')
    expect(updateState.failure).toEqual({ phase: 'check', request: offline })
  })

  it('logs an offline laptop once across ticks, and again once a check has landed in between', async () => {
    checkForUpdateMock.mockRejectedValue(new ServerRequestFailure(offline))
    await checkForUpdates('poll')
    await checkForUpdates('poll')
    expect(logger.warn).toHaveBeenCalledOnce()

    checkForUpdateMock.mockResolvedValueOnce({ kind: 'upToDate' })
    await checkForUpdates('poll')
    expect(updateState.failure).toBeNull()

    await checkForUpdates('poll')
    expect(logger.warn).toHaveBeenCalledTimes(2)
    expect(logger.error).not.toHaveBeenCalled()
  })

  it('logs a manifest the server served but this build can’t read at error, since the contract broke', async () => {
    checkForUpdateMock.mockRejectedValueOnce(
      new ServerRequestFailure({ type: 'badResponse', detail: 'missing field `platforms` at line 1 column 20' }),
    )

    await checkForUpdates('poll')

    expect(logger.error).toHaveBeenCalledOnce()
    expect(logger.warn).not.toHaveBeenCalled()
  })
})

/**
 * Download and install failures go through the same level rule as the check: a download the network or the host's
 * bad moment stopped stays at warn, so it never auto-sends an error report, while a signature mismatch, a disk
 * failure, or an install that broke stays at error.
 */
describe('a failed update download or install', () => {
  const anUpdate = { kind: 'available', version: '0.29.0' } as const

  beforeEach(() => {
    _resetUpdaterStateForTest()
    vi.clearAllMocks()
    checkForUpdateMock.mockResolvedValue(anUpdate)
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('logs a download the network stopped at warn', async () => {
    downloadUpdateMock.mockRejectedValueOnce(new UpdateDownloadFailure({ type: 'request', failure: offline }))

    await checkForUpdates('poll')

    expect(logger.warn).toHaveBeenCalledOnce()
    expect(logger.error).not.toHaveBeenCalled()
    expect(updateState.failure).toEqual({ phase: 'download' })
  })

  it('logs a download the host turned down with a 404 at error, since the release is missing', async () => {
    downloadUpdateMock.mockRejectedValueOnce(
      new UpdateDownloadFailure({ type: 'request', failure: { type: 'refused', status: 404, detail: '' } }),
    )

    await checkForUpdates('poll')

    expect(logger.error).toHaveBeenCalledOnce()
  })

  it('logs a signature mismatch at error', async () => {
    downloadUpdateMock.mockRejectedValueOnce(
      new UpdateDownloadFailure({ type: 'signatureMismatch', detail: 'Signature verification didn’t pass' }),
    )

    await checkForUpdates('poll')

    expect(logger.error).toHaveBeenCalledOnce()
    expect(logger.warn).not.toHaveBeenCalled()
  })

  it('logs a failed install at error', async () => {
    installUpdateMock.mockRejectedValueOnce(new Error('rename into the bundle: permission denied'))

    await checkForUpdates('poll')

    expect(logger.error).toHaveBeenCalledOnce()
    expect(updateState.failure).toEqual({ phase: 'install' })
  })
})
