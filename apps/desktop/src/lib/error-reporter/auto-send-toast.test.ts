/**
 * Unit tests for the Flow B auto-send toast listener.
 *
 * The listener subscribes to the Tauri `error-report-auto-sent` event, stashes the
 * report ID into the toast component's module-level `$state`, and pushes a toast via
 * `addToast`. We test the bridge: that the listener registers, dispatches on event,
 * and tears down cleanly.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { listen } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'

import { getLastAutoSentReportId } from './AutoSendToastContent.svelte'
import { initAutoSendToastListener, cleanupAutoSendToastListener } from './auto-send-toast.svelte'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
}))

type Handler = (event: { payload: string }) => void

describe('auto-send toast listener', () => {
  let registeredHandler: Handler | undefined
  const unlisten = vi.fn()

  beforeEach(() => {
    // Tear down any listener leftover from a prior test BEFORE clearing call counts;
    // otherwise the prior test's `init` stays subscribed and inflates this test's
    // `unlisten` count by one.
    cleanupAutoSendToastListener()
    vi.clearAllMocks()
    registeredHandler = undefined
    /* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-argument -- mock signature */
    const impl = ((event: string, handler: Handler) => {
      expect(event).toBe('error-report-auto-sent')
      registeredHandler = handler
      return Promise.resolve(unlisten)
    }) as any
    vi.mocked(listen).mockImplementation(impl)
    /* eslint-enable @typescript-eslint/no-explicit-any, @typescript-eslint/no-unsafe-assignment, @typescript-eslint/no-unsafe-argument */
  })

  it('registers exactly one Tauri listener on init', async () => {
    await initAutoSendToastListener()
    expect(listen).toHaveBeenCalledTimes(1)
    // Idempotent: a second init is a no-op.
    await initAutoSendToastListener()
    expect(listen).toHaveBeenCalledTimes(1)
  })

  it('shows a toast and stashes the ID when the event fires', async () => {
    await initAutoSendToastListener()
    expect(registeredHandler).toBeDefined()
    registeredHandler?.({ payload: 'ERR-LSTN1' })
    expect(addToast).toHaveBeenCalledTimes(1)
    const opts = vi.mocked(addToast).mock.calls[0][1]
    expect(opts).toMatchObject({
      id: 'error-report-auto-sent',
      level: 'info',
      dismissal: 'transient',
      timeoutMs: 10_000,
    })
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    expect(getLastAutoSentReportId()).toBe('ERR-LSTN1')
  })

  it('cleanup unregisters the listener', async () => {
    await initAutoSendToastListener()
    cleanupAutoSendToastListener()
    expect(unlisten).toHaveBeenCalledTimes(1)
    // After cleanup, init again registers a fresh listener.
    await initAutoSendToastListener()
    expect(listen).toHaveBeenCalledTimes(2)
  })
})
