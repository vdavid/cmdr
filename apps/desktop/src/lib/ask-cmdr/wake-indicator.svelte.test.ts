/**
 * What the status corner is allowed to say about the proactive agent.
 *
 * The gate is the whole point of this module: it is where two rules that used to contradict each
 * other in two different files got reconciled. `agent/wake/readiness.rs` holds that every gap is
 * worth reporting, because a declined disk-access prompt and a tidy Downloads folder otherwise
 * look identical. `SuggestedOpsIndicator` holds that a control for a feature with nothing to say
 * is noise. The resolution is that a gap is reported to somebody who opted IN, and to nobody else.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  openWakeThread,
  startWakeIndicator,
  stopWake,
  stopWakeIndicator,
  wakeIndicator,
  wakeIndicatorMode,
} from './wake-indicator.svelte'

const { commands } = vi.hoisted(() => ({
  commands: {
    agentWakeStatus: vi.fn(),
    cancelAskCmdr: vi.fn<(id: number) => Promise<void>>(() => Promise.resolve()),
    onAgentWakeStatus: vi.fn(),
  },
}))
const { rail } = vi.hoisted(() => ({
  rail: {
    switchToThread: vi.fn<(id: number) => Promise<void>>(() => Promise.resolve()),
    openRail: vi.fn<() => Promise<void>>(() => Promise.resolve()),
  },
}))

vi.mock('$lib/tauri-commands', () => commands)
vi.mock('./ask-cmdr-trigger.svelte', () => rail)
vi.mock('$lib/settings', () => ({
  getSetting: () => true,
  onSpecificSettingChange: () => () => {},
}))

beforeEach(() => {
  vi.clearAllMocks()
  stopWakeIndicator()
  wakeIndicator.thinkingIn = null
  wakeIndicator.readiness = 'needsConsent'
  wakeIndicator.proactive = false
})

describe('what the corner shows', () => {
  it('says nothing to somebody who never opted in, whatever the gates report', () => {
    for (const readiness of ['ready', 'needsConsent', 'needsFullDiskAccess', 'needsApiKey'] as const) {
      wakeIndicator.readiness = readiness
      expect(wakeIndicatorMode(wakeIndicator)).toBe('silent')
    }
  })

  it('still says nothing once they opt in but consent is missing', () => {
    wakeIndicator.proactive = true
    wakeIndicator.readiness = 'needsConsent'

    expect(wakeIndicatorMode(wakeIndicator)).toBe('silent')
  })

  it('stays quiet while the agent is ready and idle: there is nothing to report', () => {
    wakeIndicator.proactive = true
    wakeIndicator.readiness = 'ready'

    expect(wakeIndicatorMode(wakeIndicator)).toBe('silent')
  })

  it('names the gap for somebody who opted in and hit a wall', () => {
    wakeIndicator.proactive = true

    wakeIndicator.readiness = 'needsFullDiskAccess'
    expect(wakeIndicatorMode(wakeIndicator)).toBe('needsFullDiskAccess')

    wakeIndicator.readiness = 'needsApiKey'
    expect(wakeIndicatorMode(wakeIndicator)).toBe('needsApiKey')
  })

  it('shows a running wake even with the setting off, because it is spending money right now', () => {
    // A forced wake, or a setting turned off mid-turn. Hiding the turn would leave the user
    // with no way to see it or stop it.
    wakeIndicator.proactive = false
    wakeIndicator.readiness = 'needsConsent'
    wakeIndicator.thinkingIn = 42

    expect(wakeIndicatorMode(wakeIndicator)).toBe('thinking')
  })

  it('lets a running wake win over a gap, since the wake is the more urgent fact', () => {
    wakeIndicator.proactive = true
    wakeIndicator.readiness = 'needsApiKey'
    wakeIndicator.thinkingIn = 7

    expect(wakeIndicatorMode(wakeIndicator)).toBe('thinking')
  })
})

describe('the subscription', () => {
  it('seeds from the backend, for the wake already running when the window opened', async () => {
    commands.agentWakeStatus.mockResolvedValue({
      phase: { phase: 'thinking', conversationId: 11 },
      readiness: 'ready',
    })
    commands.onAgentWakeStatus.mockResolvedValue(() => {})

    await startWakeIndicator()

    expect(wakeIndicator.thinkingIn).toBe(11)
    expect(wakeIndicator.readiness).toBe('ready')
  })

  it('clears the thread on the idle phase, so no stale click target survives a quiet wake', async () => {
    let emit: ((payload: unknown) => void) | undefined
    commands.agentWakeStatus.mockResolvedValue({ phase: { phase: 'idle' }, readiness: 'ready' })
    commands.onAgentWakeStatus.mockImplementation((cb: (payload: unknown) => void) => {
      emit = cb
      return Promise.resolve(() => {})
    })
    await startWakeIndicator()

    emit?.({ phase: { phase: 'thinking', conversationId: 5 }, readiness: 'ready' })
    expect(wakeIndicator.thinkingIn).toBe(5)

    emit?.({ phase: { phase: 'idle' }, readiness: 'ready' })
    expect(wakeIndicator.thinkingIn).toBeNull()
  })

  it('stays silent rather than guessing when the seed read throws', async () => {
    commands.agentWakeStatus.mockRejectedValue(new Error('no store'))
    commands.onAgentWakeStatus.mockResolvedValue(() => {})

    await startWakeIndicator()

    expect(wakeIndicatorMode(wakeIndicator)).toBe('silent')
  })
})

describe('the two actions', () => {
  it('loads the thread BEFORE opening the rail, so the open does not bootstrap another one', async () => {
    wakeIndicator.thinkingIn = 9
    const order: string[] = []
    rail.switchToThread.mockImplementation(() => {
      order.push('switch')
      return Promise.resolve()
    })
    rail.openRail.mockImplementation(() => {
      order.push('open')
      return Promise.resolve()
    })

    await openWakeThread()

    expect(order).toEqual(['switch', 'open'])
    expect(rail.switchToThread).toHaveBeenCalledWith(9)
  })

  it('stops the wake through the ONE cancel command, keyed on its thread', async () => {
    wakeIndicator.thinkingIn = 3

    await stopWake()

    expect(commands.cancelAskCmdr).toHaveBeenCalledWith(3)
  })

  it('does nothing when no wake is running, so a late click cannot cancel the next one', async () => {
    wakeIndicator.thinkingIn = null

    await openWakeThread()
    await stopWake()

    expect(rail.switchToThread).not.toHaveBeenCalled()
    expect(commands.cancelAskCmdr).not.toHaveBeenCalled()
  })
})
