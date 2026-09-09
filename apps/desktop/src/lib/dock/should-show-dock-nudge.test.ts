/**
 * When Cmdr may ask to be kept in the Dock, and when it must stay quiet.
 *
 * The once-ever stamp makes every row here a one-shot: a decision that fires on
 * the wrong launch can't be taken back, and one that never fires is a feature
 * nobody is offered. The gates this shares with the reveal offer are covered in
 * `$lib/nudges/nudge-ledger.test.ts`; these rows are the Dock's own.
 */

import { describe, it, expect } from 'vitest'
import type { DockPinState } from '$lib/ipc/bindings'
import { emptyNudgeLedger } from '$lib/nudges/nudge-ledger'
import { shouldShowDockNudge, DOCK_NUDGE_AFTER_DAYS, type DockNudgeInputs } from './should-show-dock-nudge'

const NOW = new Date('2026-09-09T12:00:00Z')

/** `days` before {@link NOW}, as the ISO instant the nudge ledger stores. */
function daysAgo(days: number): string {
  return new Date(NOW.getTime() - days * 24 * 60 * 60 * 1000).toISOString()
}

/** A settled Mac on its fourth day, with room in the Dock: the one case that speaks up. */
const ready: DockNudgeInputs = {
  automatedRun: false,
  onMacOs: true,
  onboarded: true,
  onboardingShowing: false,
  ledger: emptyNudgeLedger(),
  now: NOW,
  launchDayCount: DOCK_NUDGE_AFTER_DAYS,
  pinState: { kind: 'offerable' } satisfies DockPinState,
}

describe('shouldShowDockNudge', () => {
  it('offers the pin once the ledger has enough days', () => {
    expect(shouldShowDockNudge(ready)).toBe(true)
  })

  it('stays quiet on the launch before the threshold', () => {
    expect(shouldShowDockNudge({ ...ready, launchDayCount: DOCK_NUDGE_AFTER_DAYS - 1 })).toBe(false)
  })

  /**
   * "At least four days", ❌ not "the fourth day is today": the ledger started
   * counting when it shipped, and someone who has been here for months is
   * exactly who the offer is for.
   */
  it('offers the pin to a long-standing user, not only on the day it tips over', () => {
    expect(shouldShowDockNudge({ ...ready, launchDayCount: 400 })).toBe(true)
  })

  it('never asks twice, however long the ledger gets', () => {
    const ledger = { dockPin: daysAgo(900), reveal: '' }
    expect(shouldShowDockNudge({ ...ready, ledger, launchDayCount: 400 })).toBe(false)
  })

  /**
   * The known interaction, and it's the cooldown working as intended: a daily
   * user hears about the reveal handler on launch day two, so the Dock offer
   * lands three days after that rather than exactly on launch day four. ❌ Don't
   * exempt the Dock from the floor to "fix" this.
   */
  it('waits three days after the reveal offer rather than firing on its own day', () => {
    const ledger = { dockPin: '', reveal: daysAgo(1) }
    expect(shouldShowDockNudge({ ...ready, ledger, launchDayCount: 400 })).toBe(false)
  })

  it('says nothing while Cmdr is already down there', () => {
    expect(shouldShowDockNudge({ ...ready, pinState: { kind: 'alreadyPinned' } })).toBe(false)
  })

  it.each([
    ['notABundle', 'a dev build with no `.app` around it'],
    ['outsideApplications', 'a copy running from somewhere a tile would soon point nowhere'],
    ['managedDock', 'a Dock a configuration profile owns'],
    ['preferencesUnreadable', 'a Dock whose preferences we could not read'],
  ] as const)('says nothing about %s (%s)', (reason, _why) => {
    expect(shouldShowDockNudge({ ...ready, pinState: { kind: 'unavailable', reason } })).toBe(false)
  })

  it('stays quiet under an automated run, so it cannot leak into the first spec', () => {
    expect(shouldShowDockNudge({ ...ready, automatedRun: true })).toBe(false)
  })

  it('stays quiet off macOS, where there is no Dock to be in', () => {
    expect(shouldShowDockNudge({ ...ready, onMacOs: false })).toBe(false)
  })

  it('waits for onboarding to be finished', () => {
    expect(shouldShowDockNudge({ ...ready, onboarded: false })).toBe(false)
  })

  it('waits for the wizard to leave the screen', () => {
    expect(shouldShowDockNudge({ ...ready, onboardingShowing: true })).toBe(false)
  })
})
