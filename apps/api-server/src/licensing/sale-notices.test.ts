import { afterEach, describe, expect, it, vi } from 'vitest'
import { notifySale } from './sale-notices'
import type { Bindings } from '../types'

/**
 * The gates in front of the sale notice, which decide before anything leaves the Worker. The
 * runtime suite (`sale-notice-runtime.test.ts`) runs under the production `PADDLE_ENVIRONMENT`, so
 * the sandbox gate is pinned here.
 */
function env(overrides: Partial<Bindings>): Bindings {
  return {
    PADDLE_ENVIRONMENT: 'live',
    PADDLE_API_KEY_LIVE: 'key',
    PADDLE_API_KEY_SANDBOX: 'key',
    CRASH_NOTIFICATION_EMAIL: 'david@example.com',
    RESEND_API_KEY: 'resend',
    ...overrides,
  } as Bindings
}

afterEach(() => {
  vi.restoreAllMocks()
})

describe('notifySale', () => {
  it('stays quiet for a sandbox purchase, without asking Paddle anything', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')

    expect(await notifySale(env({ PADDLE_ENVIRONMENT: 'sandbox' }), 'txn_sandbox')).toBe('not_live')
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  it('stays quiet with nobody configured to tell', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch')

    expect(await notifySale(env({ CRASH_NOTIFICATION_EMAIL: undefined }), 'txn_nobody')).toBe('no_recipient')
    expect(fetchSpy).not.toHaveBeenCalled()
  })

  it('answers failed rather than throwing when Paddle is down', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('network down'))
    vi.spyOn(console, 'error').mockImplementation(() => undefined)

    expect(await notifySale(env({}), 'txn_paddle_down')).toBe('failed')
  })
})
