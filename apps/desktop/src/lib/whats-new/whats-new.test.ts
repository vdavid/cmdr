/**
 * Unit tests for the pure `decideWhatsNew` trigger helper. No IPC, no `$state`: this
 * module is deliberately pure so the full decision truth table is fast and deterministic
 * to cover. The `compareVersions` comparator it leans on is covered in
 * `$lib/utils/version.test.ts`.
 */

import { describe, it, expect } from 'vitest'
import { decideWhatsNew } from './whats-new'

const base = {
  enabled: true,
  onboarded: true,
  onboardingShowing: false,
  otherStartupModalOpen: false,
}

describe('decideWhatsNew', () => {
  it('fresh install (no lastSeen, not onboarded): silent stamp, never a popup', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '', current: '0.26.0', onboarded: false })
    expect(result).toEqual({ action: 'stamp' })
  })

  it('inaugural showcase (no lastSeen, onboarded, enabled): show current only', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '', current: '0.26.0' })
    expect(result).toEqual({ action: 'show', since: null, max: 1 })
  })

  it('inaugural showcase but feature disabled: stamp silently', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '', current: '0.26.0', enabled: false })
    expect(result).toEqual({ action: 'stamp' })
  })

  it('upgrade with feature on: show the diff from lastSeen, capped at five', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '0.20.0', current: '0.26.0' })
    expect(result).toEqual({ action: 'show', since: '0.20.0', max: 5 })
  })

  it('upgrade with feature off: stamp silently (no backlog on re-enable)', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '0.20.0', current: '0.26.0', enabled: false })
    expect(result).toEqual({ action: 'stamp' })
  })

  it('version unchanged: do nothing', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '0.26.0', current: '0.26.0' })
    expect(result).toEqual({ action: 'none' })
  })

  it('downgrade: rewrite lastSeen to current, no popup', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '0.27.0', current: '0.26.0' })
    expect(result).toEqual({ action: 'stamp' })
  })

  it('would-show but onboarding is up: wait, do not stamp', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '0.20.0', current: '0.26.0', onboardingShowing: true })
    expect(result).toEqual({ action: 'wait' })
  })

  it('would-show but another startup modal is up: wait, do not stamp', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '0.20.0', current: '0.26.0', otherStartupModalOpen: true })
    expect(result).toEqual({ action: 'wait' })
  })

  it('inaugural showcase blocked by a modal: wait', () => {
    const result = decideWhatsNew({ ...base, lastSeen: '', current: '0.26.0', onboardingShowing: true })
    expect(result).toEqual({ action: 'wait' })
  })

  it('silent stamp paths run even while a modal is up (downgrade)', () => {
    // Modal gating only blocks the `show` actions; the silent stamp must run regardless.
    const result = decideWhatsNew({
      ...base,
      lastSeen: '0.27.0',
      current: '0.26.0',
      otherStartupModalOpen: true,
    })
    expect(result).toEqual({ action: 'stamp' })
  })

  it('silent stamp paths run even while a modal is up (feature disabled upgrade)', () => {
    const result = decideWhatsNew({
      ...base,
      lastSeen: '0.20.0',
      current: '0.26.0',
      enabled: false,
      onboardingShowing: true,
    })
    expect(result).toEqual({ action: 'stamp' })
  })

  it('double-digit upgrade is treated as an increase, not a downgrade', () => {
    // 0.9.0 < 0.10.0: a string compare would read this as a downgrade and silently stamp.
    const result = decideWhatsNew({ ...base, lastSeen: '0.9.0', current: '0.10.0' })
    expect(result).toEqual({ action: 'show', since: '0.9.0', max: 5 })
  })
})
