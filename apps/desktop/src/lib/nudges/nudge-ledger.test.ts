/**
 * The shared nudge rule: when an unsolicited offer may speak, and when another
 * one having just spoken keeps it quiet.
 *
 * Every offer is once-ever, so each row here is a one-shot: a nudge that fires
 * on the wrong launch can't be taken back.
 */

import { describe, it, expect } from 'vitest'
import {
  NUDGE_COOLDOWN_DAYS,
  emptyNudgeLedger,
  nudgeCooldownActive,
  nudgeCouldFire,
  wasOffered,
  type NudgeContext,
} from './nudge-ledger'

const NOW = new Date('2026-09-09T12:00:00Z')

/** `days` before {@link NOW}, as the ISO instant the ledger stores. */
function daysAgo(days: number): string {
  return new Date(NOW.getTime() - days * 24 * 60 * 60 * 1000).toISOString()
}

/** A settled Mac that has never been nudged: the one case that speaks up. */
const ready: NudgeContext = {
  automatedRun: false,
  onMacOs: true,
  onboarded: true,
  onboardingShowing: false,
  ledger: emptyNudgeLedger(),
  now: NOW,
}

describe('wasOffered', () => {
  it('reads an empty stamp as "never asked"', () => {
    expect(wasOffered('')).toBe(false)
  })

  it('reads any stamp as "already asked"', () => {
    expect(wasOffered(daysAgo(900))).toBe(true)
  })

  /**
   * A stamp nothing can parse still means the offer happened; only WHEN is
   * lost. Reading it as "never asked" would offer a once-ever thing twice.
   */
  it('treats an unparseable stamp as asked, so a bad clock cannot re-offer', () => {
    expect(wasOffered('not-a-date')).toBe(true)
  })
})

describe('nudgeCooldownActive', () => {
  it('is quiet when nothing has ever been offered', () => {
    expect(nudgeCooldownActive(emptyNudgeLedger(), NOW)).toBe(false)
  })

  it('holds the floor for another nudge made today', () => {
    expect(nudgeCooldownActive({ dockPin: daysAgo(0), reveal: '' }, NOW)).toBe(true)
  })

  it('still holds the day before the cooldown runs out', () => {
    expect(nudgeCooldownActive({ dockPin: '', reveal: daysAgo(NUDGE_COOLDOWN_DAYS - 1) }, NOW)).toBe(true)
  })

  it('lets go once the cooldown has passed', () => {
    expect(nudgeCooldownActive({ dockPin: daysAgo(NUDGE_COOLDOWN_DAYS), reveal: '' }, NOW)).toBe(false)
  })

  it('takes the most recent nudge, not the first one it finds', () => {
    expect(nudgeCooldownActive({ dockPin: daysAgo(400), reveal: daysAgo(1) }, NOW)).toBe(true)
  })

  /**
   * A stamp we can't read, or one from the future because the clock moved
   * backwards, must not silence every remaining nudge forever. Blocking is the
   * job of a stamp we can actually reason about.
   */
  it.each([
    ['an unreadable stamp', 'whenever'],
    ['a stamp from the future', new Date(NOW.getTime() + 60_000).toISOString()],
  ])('ignores %s rather than going quiet forever', (_case, stamp) => {
    expect(nudgeCooldownActive({ dockPin: stamp, reveal: '' }, NOW)).toBe(false)
  })
})

describe('nudgeCouldFire', () => {
  it('lets a settled Mac through, so the gate goes on to ask the backend', () => {
    expect(nudgeCouldFire(ready, 'reveal')).toBe(true)
  })

  it.each([
    ['an automated run', { automatedRun: true }],
    ['a machine with no Dock and no NSFileViewer', { onMacOs: false }],
    ['onboarding still unfinished', { onboarded: false }],
    ['the wizard on screen', { onboardingShowing: true }],
  ] as const)('turns around on %s, before any IPC is paid for', (_case, override) => {
    expect(nudgeCouldFire({ ...ready, ...override }, 'reveal')).toBe(false)
  })

  it('never asks the same thing twice', () => {
    expect(nudgeCouldFire({ ...ready, ledger: { dockPin: '', reveal: daysAgo(900) } }, 'reveal')).toBe(false)
  })

  /**
   * The whole point of one shared floor: two offers days apart, never two in
   * the same week. The Dock offer waits its turn rather than earning an
   * exemption.
   */
  it('holds one nudge back while another one is still fresh', () => {
    const ledger = { dockPin: '', reveal: daysAgo(1) }
    expect(nudgeCouldFire({ ...ready, ledger }, 'dockPin')).toBe(false)
  })

  it('lets the next nudge through once the floor has passed', () => {
    const ledger = { dockPin: '', reveal: daysAgo(NUDGE_COOLDOWN_DAYS) }
    expect(nudgeCouldFire({ ...ready, ledger }, 'dockPin')).toBe(true)
  })
})
