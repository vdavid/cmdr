/**
 * Tier 3 a11y tests for `SmbReconnectingView.svelte`.
 *
 * Covers the three cycle states (waiting, attempting, gave-up; but the pane
 * never renders the gave-up state itself; the parent swaps to
 * `VolumeUnreachableBanner`). Validates structural a11y in each phase and that
 * the buttons stay accessible when "Retry now" is disabled mid-attempt.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SmbReconnectingView from './SmbReconnectingView.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { RECONNECT_DELAYS_MS, type ReconnectState } from '../network/smb-reconnect-manager.svelte'

vi.mock('$lib/tauri-commands', () => ({
  reconnectSmbVolume: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

function waitingState(attemptIndex = 0): ReconnectState {
  return {
    status: 'waiting',
    attemptIndex,
    currentDelayMs: RECONNECT_DELAYS_MS[attemptIndex],
    waitStartedAt: performance.now(),
  }
}

function attemptingState(attemptIndex = 0): ReconnectState {
  return {
    status: 'attempting',
    attemptIndex,
    currentDelayMs: RECONNECT_DELAYS_MS[attemptIndex],
    waitStartedAt: performance.now(),
  }
}

describe('SmbReconnectingView a11y', () => {
  it('first wait (no body 2) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbReconnectingView, {
      target,
      props: {
        volumeId: 'volumesnaspi',
        shareName: 'naspi',
        cycleState: waitingState(0),
        onCancel: () => {},
        onDisconnect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('mid-cycle wait with body 2 has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbReconnectingView, {
      target,
      props: {
        volumeId: 'volumesnaspi',
        shareName: 'naspi',
        cycleState: waitingState(2),
        onCancel: () => {},
        onDisconnect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('attempting state (Retry now disabled) has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbReconnectingView, {
      target,
      props: {
        volumeId: 'volumesnaspi',
        shareName: 'naspi',
        cycleState: attemptingState(1),
        onCancel: () => {},
        onDisconnect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
