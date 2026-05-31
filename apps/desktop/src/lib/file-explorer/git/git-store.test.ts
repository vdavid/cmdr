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

  it('coalesces concurrent subscribes for the same repo into one backend call', async () => {
    // Hold every `subscribe_git_state` invocation on a deferred we release
    // together, so two concurrent `subscribeToRepo` calls fully interleave
    // across their awaits before any backend response lands. Resolving all of
    // them (not just the first) means buggy code that issues a second backend
    // subscribe still settles — so the test fails on the call-count assertion
    // rather than hanging.
    const pending: Array<(info: typeof baseInfo) => void> = []
    vi.mocked(invoke).mockImplementation((name: string) => {
      if (name === 'subscribe_git_state') {
        return new Promise((resolve) => {
          pending.push(resolve)
        })
      }
      if (name === 'unsubscribe_git_state') return Promise.resolve(undefined)
      return Promise.resolve(baseInfo)
    })

    // Fire two concurrent subscribes for the same root before either resolves.
    const p1 = subscribeToRepo('/repo')
    const p2 = subscribeToRepo('/repo')

    // Let both run up to their first pending backend await.
    await Promise.resolve()
    await Promise.resolve()

    // Release every in-flight backend subscribe.
    for (const resolve of pending) resolve(baseInfo)
    const [a, b] = await Promise.all([p1, p2])

    expect(a.repoRoot).toBe('/repo')
    expect(b.repoRoot).toBe('/repo')

    // The backend subscribe must fire exactly once even though two callers raced.
    const subscribeCalls = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'subscribe_git_state')
    expect(subscribeCalls.length).toBe(1)

    // Two subscribers means refcount 2: the first unsubscribe must NOT tear down.
    await unsubscribeFromRepo('/repo')
    let unsub = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'unsubscribe_git_state')
    expect(unsub.length).toBe(0)
    expect(getRepoInfo('/repo')).not.toBeNull()

    // The second unsubscribe drops refcount to 0 and tears the backend down.
    await unsubscribeFromRepo('/repo')
    unsub = vi.mocked(invoke).mock.calls.filter(([name]) => name === 'unsubscribe_git_state')
    expect(unsub.length).toBe(1)
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
