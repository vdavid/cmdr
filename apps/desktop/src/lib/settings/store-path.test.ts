/**
 * Tests for the generic store path resolver.
 *
 * The store path must redirect to the backend-supplied isolated path when one
 * is returned (dev / per-worktree dev / E2E), and stay byte-identical to the
 * bare store name in production (backend returns `null`). A rejected name
 * (traversal / absolute, which the backend maps to `null`) must also fall back
 * to the bare name so it can never escape the data dir.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getIsolatedStorePath: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { resolveStorePath } from './store-path'

describe('resolveStorePath', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns the isolated path when the backend supplies one', async () => {
    const isolated = '/tmp/cmdr-e2e-data-mtp-123/shortcuts.json'
    vi.mocked(commands.getIsolatedStorePath).mockResolvedValueOnce(isolated)
    expect(await resolveStorePath('shortcuts.json')).toBe(isolated)
    expect(commands.getIsolatedStorePath).toHaveBeenCalledWith('shortcuts.json')
  })

  it('falls back to the bare store name in production (backend returns null)', async () => {
    vi.mocked(commands.getIsolatedStorePath).mockResolvedValueOnce(null)
    expect(await resolveStorePath('settings.json')).toBe('settings.json')
  })

  it('falls back to the bare store name when the backend rejects the name (null)', async () => {
    // The backend maps traversal / absolute names to `null`; the FE treats that
    // exactly like production, so a bad name can never escape the data dir.
    vi.mocked(commands.getIsolatedStorePath).mockResolvedValueOnce(null)
    expect(await resolveStorePath('../../escape.json')).toBe('../../escape.json')
  })

  it('falls back to the bare store name and reports when the command rejects', async () => {
    const err = new Error('boom')
    vi.mocked(commands.getIsolatedStorePath).mockRejectedValueOnce(err)
    const onError = vi.fn()
    expect(await resolveStorePath('app-status.json', onError)).toBe('app-status.json')
    expect(onError).toHaveBeenCalledWith(err)
  })

  it('does not require an onError callback when the command rejects', async () => {
    vi.mocked(commands.getIsolatedStorePath).mockRejectedValueOnce(new Error('boom'))
    expect(await resolveStorePath('settings.json')).toBe('settings.json')
  })
})
