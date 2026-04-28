/**
 * Tests for the per-repo git state store.
 *
 * The store wraps a Tauri event listener and IPC subscribe/unsubscribe calls.
 * Tests mock both layers and assert the refcount + caching behaviour we
 * promise to `FilePane.svelte`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { subscribeToRepo, unsubscribeFromRepo, lookupRepoInfo, getRepoInfo } from './git-store.svelte'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

const baseInfo = {
  repoRoot: '/repo',
  branch: 'main',
  detachedSha: null,
  unborn: false,
  upstream: null,
  ahead: null,
  behind: null,
  isDirty: false,
}

describe('git-store', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
    vi.mocked(listen).mockClear()
  })

  it('subscribes once per repo and refcounts repeat callers', async () => {
    vi.mocked(invoke).mockResolvedValue(baseInfo)

    const a = await subscribeToRepo('/repo')
    const b = await subscribeToRepo('/repo')

    expect(a.repoRoot).toBe('/repo')
    expect(b.repoRoot).toBe('/repo')
    // Subscribe IPC fires only once; the second call hits the cached entry.
    const subscribeCalls = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'subscribe_git_state')
    expect(subscribeCalls.length).toBe(1)

    await unsubscribeFromRepo('/repo')
    // First refcount drop doesn't tear down.
    const unsubCallsAfterFirst = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'unsubscribe_git_state')
    expect(unsubCallsAfterFirst.length).toBe(0)

    await unsubscribeFromRepo('/repo')
    const unsubCallsAfterSecond = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'unsubscribe_git_state')
    expect(unsubCallsAfterSecond.length).toBe(1)
    expect(getRepoInfo('/repo')).toBeNull()
  })

  it('lookupRepoInfo unwraps `data` from the timeout-aware envelope', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ data: baseInfo, timedOut: false })
    const result = await lookupRepoInfo('/some/path')
    expect(result?.repoRoot).toBe('/repo')
  })

  it('lookupRepoInfo returns null when the backend has no repo for the path', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ data: null, timedOut: false })
    const result = await lookupRepoInfo('/no/repo/here')
    expect(result).toBeNull()
  })

  it('getRepoInfo is null until the first subscribe returns', () => {
    expect(getRepoInfo('/never-subscribed')).toBeNull()
  })

  it('unsubscribeFromRepo is a no-op for unknown roots', async () => {
    await unsubscribeFromRepo('/never-subscribed')
    const unsub = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'unsubscribe_git_state')
    expect(unsub.length).toBe(0)
  })
})
