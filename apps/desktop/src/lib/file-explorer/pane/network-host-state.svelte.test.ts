/**
 * Tests for `network-host-state.svelte.ts`, the host (and pending auto-mount
 * share) a pane remembers while it's on the Network volume. They pin the rule
 * that only an E2E test covered before: leaving the network volume by ANY route
 * clears both, so re-entering Network lands on the host list rather than a stale
 * share browser.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { NetworkHost } from '../types'
import { createNetworkHostState, type NetworkHostState } from './network-host-state.svelte'

const host = { id: 'nas', name: 'Naspolya' } as unknown as NetworkHost

describe('createNetworkHostState', () => {
  let dispose: (() => void) | undefined

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  function create(startsOnNetwork = true) {
    let isNetworkView = $state(startsOnNetwork)
    const onHostChange = vi.fn()
    let state!: NetworkHostState
    dispose = $effect.root(() => {
      state = createNetworkHostState({ getIsNetworkView: () => isNetworkView, onHostChange })
    })
    flushSync()
    return {
      state,
      onHostChange,
      leaveNetwork: () => {
        isNetworkView = false
        flushSync()
      },
    }
  }

  it('remembers the host the user opened', () => {
    const { state } = create()
    state.setHost(host)
    flushSync()
    // `$state` proxies the object, so compare by value (as it was on the pane before).
    expect(state.host).toStrictEqual(host)
  })

  it('bubbles a host change out for history tracking', () => {
    const created = create()
    created.state.handleHostChange(host)
    expect(created.onHostChange).toHaveBeenCalledWith(host)
    expect(created.state.host).toStrictEqual(host)
  })

  it('queues a share to auto-mount', () => {
    const { state } = create()
    state.setAutoMountShare('media')
    flushSync()
    expect(state.autoMountShare).toBe('media')
  })

  it('clears both when the pane leaves the network volume', () => {
    const created = create()
    created.state.setHost(host)
    created.state.setAutoMountShare('media')
    flushSync()

    created.leaveNetwork()
    expect(created.state.host).toBeNull()
    expect(created.state.autoMountShare).toBeUndefined()
  })

  it('starts clean on a pane that was never on the network volume', () => {
    const { state } = create(false)
    expect(state.host).toBeNull()
    expect(state.autoMountShare).toBeUndefined()
  })
})
