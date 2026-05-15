/**
 * Tests for the FE breadcrumb wrapper.
 *
 * The wrapper is intentionally thin: it just calls `invoke('record_breadcrumb', ...)`
 * with a normalised `ctx` (null when undefined) and swallows errors. We assert the
 * IPC contract because callers rely on it (kind, message, ctx shape).
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { recordBreadcrumb } from './breadcrumbs'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve()),
}))

const mockedInvoke = vi.mocked(invoke)

describe('recordBreadcrumb', () => {
  beforeEach(() => {
    mockedInvoke.mockClear()
    mockedInvoke.mockImplementation(() => Promise.resolve())
  })

  it('forwards kind, message, and ctx to the Tauri command', () => {
    recordBreadcrumb('nav', 'to /Users', { from: '/old', to: '/Users' })
    expect(mockedInvoke).toHaveBeenCalledWith('record_breadcrumb', {
      kind: 'nav',
      message: 'to /Users',
      ctx: { from: '/old', to: '/Users' },
    })
  })

  it('passes ctx as null when omitted (Rust expects Option<Value>)', () => {
    recordBreadcrumb('command', 'app.quit')
    expect(mockedInvoke).toHaveBeenCalledWith('record_breadcrumb', {
      kind: 'command',
      message: 'app.quit',
      ctx: null,
    })
  })

  it('swallows errors so breadcrumb failures never break the UI', async () => {
    mockedInvoke.mockRejectedValueOnce(new Error('IPC unavailable'))
    expect(() => {
      recordBreadcrumb('test', 'will-reject')
    }).not.toThrow()
    // Let the rejection settle on the microtask queue so coverage sees the catch branch.
    await Promise.resolve()
    expect(mockedInvoke).toHaveBeenCalledOnce()
  })
})
