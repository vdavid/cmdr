import { describe, expect, it, beforeAll, afterAll, onTestFinished } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import type { DeviceReadiness } from '#lib/ipc/bindings.js'
import { deviceRowState, isStillConnecting } from './device-readiness'

describe('deviceRowState', () => {
  beforeAll(() => {
    _setLocaleForTests('en-US')
  })
  afterAll(() => {
    _setLocaleForTests(null)
  })

  it('leaves a row with no readiness alone: a disk is not a device', () => {
    expect(deviceRowState(null)).toEqual({ openable: true, tooltip: null, status: null })
    expect(deviceRowState(undefined)).toEqual({ openable: true, tooltip: null, status: null })
  })

  it('says nothing about a ready device', () => {
    expect(deviceRowState({ kind: 'ready' })).toEqual({ openable: true, tooltip: null, status: null })
  })

  // ❗ Openable, not disabled: opening it is what puts the pane on the waiting
  // state that proceeds by itself once the user taps Allow. A disabled row here
  // is the silence this whole feature exists to end.
  it('keeps a phone waiting for its Allow tap openable, and says what it is waiting for', () => {
    expect(deviceRowState({ kind: 'waiting_for_authorization' })).toEqual({
      openable: true,
      // ❗ Names WHERE the user acts and WHAT they press, ❌ not the Android
      // feature: someone staring at a greyed-looking row needs the next move.
      tooltip: 'Waiting for you to tap Allow on your phone',
      status: 'Tap Allow on your phone',
    })
  })

  // ❗ The MTP open can time out with no prompt on screen (#834's retest: a phone still
  // recovering from a pulled cable), so "if it asks" is load-bearing: ❌ never word this as
  // though a prompt is showing.
  it('words a connecting phone so it stays true when no prompt is showing', () => {
    expect(deviceRowState({ kind: 'connecting' })).toEqual({
      // No storage to land on yet, and a pane parked on the row would have nothing to walk into.
      openable: false,
      tooltip: 'Cmdr keeps trying while your phone stays plugged in, and you can open it once it connects.',
      status: 'Still connecting. Unlock your phone and tap Allow if it asks.',
    })
  })

  it.each<[DeviceReadiness, string]>([
    [{ kind: 'unavailable', reason: 'offline' }, 'Your phone isn’t responding. Wake its screen, or reseat the cable.'],
    [
      { kind: 'unavailable', reason: 'no_permissions' },
      'This Mac can’t reach your phone over USB. Try another cable or port.',
    ],
  ])('disables an unavailable device and puts the reason in its tooltip', (readiness, tooltip) => {
    onTestFinished(withPlatform('macos'))
    expect(deviceRowState(readiness)).toEqual({ openable: false, tooltip, status: null })
  })

  it.each([
    ['windows', 'This PC can’t reach your phone over USB. Try another cable or port.'],
    ['linux', 'This computer can’t reach your phone over USB. Try another cable or port.'],
  ] as const)('names the machine in %s’s words when USB can’t reach the phone', (platform, tooltip) => {
    onTestFinished(withPlatform(platform))
    expect(deviceRowState({ kind: 'unavailable', reason: 'no_permissions' }).tooltip).toBe(tooltip)
  })
})

describe('isStillConnecting', () => {
  it('answers for the connecting row alone', () => {
    expect(isStillConnecting({ deviceReadiness: { kind: 'connecting' } })).toBe(true)
    expect(isStillConnecting({ deviceReadiness: { kind: 'waiting_for_authorization' } })).toBe(false)
    expect(isStillConnecting({ deviceReadiness: { kind: 'ready' } })).toBe(false)
    expect(isStillConnecting({ deviceReadiness: null })).toBe(false)
    expect(isStillConnecting({})).toBe(false)
  })
})
