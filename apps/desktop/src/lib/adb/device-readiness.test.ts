import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { DeviceReadiness } from '$lib/ipc/bindings'
import { deviceRowState } from './device-readiness'

describe('deviceRowState', () => {
  beforeAll(() => {
    _setLocaleForTests('en-US')
  })
  afterAll(() => {
    _setLocaleForTests(null)
  })

  it('leaves a row with no readiness alone: a disk is not a device', () => {
    expect(deviceRowState(null)).toEqual({ openable: true, tooltip: null })
    expect(deviceRowState(undefined)).toEqual({ openable: true, tooltip: null })
  })

  it('says nothing about a ready device', () => {
    expect(deviceRowState({ kind: 'ready' })).toEqual({ openable: true, tooltip: null })
  })

  // ❗ Openable, not disabled: opening it is what puts the pane on the waiting
  // state that proceeds by itself once the user taps Allow. A disabled row here
  // is the silence this whole feature exists to end.
  it('keeps a phone waiting for its Allow tap openable, and says what it is waiting for', () => {
    expect(deviceRowState({ kind: 'waiting_for_authorization' })).toEqual({
      openable: true,
      tooltip: 'Waiting for you to allow USB debugging',
    })
  })

  it.each<[DeviceReadiness, string]>([
    [{ kind: 'unavailable', reason: 'offline' }, "Your phone isn't responding. Wake its screen, or reseat the cable."],
    [
      { kind: 'unavailable', reason: 'no_permissions' },
      "This Mac can't reach your phone over USB. Try another cable or port.",
    ],
  ])('disables an unavailable device and puts the reason in its tooltip', (readiness, tooltip) => {
    expect(deviceRowState(readiness)).toEqual({ openable: false, tooltip })
  })
})
