/**
 * When Cmdr may ask to be kept in the Dock, and when it must stay quiet.
 *
 * The once-ever flag makes every row here a one-shot: a decision that fires on
 * the wrong launch can't be taken back, and one that never fires is a feature
 * nobody is offered.
 */

import { describe, it, expect } from 'vitest'
import type { DockPinState } from '$lib/ipc/bindings'
import { shouldShowDockNudge, dockNudgeCouldFire, DOCK_NUDGE_AFTER_DAYS } from './should-show-dock-nudge'

/** A settled Mac on its third day, with room in the Dock: the one case that speaks up. */
const ready = {
  automatedRun: false,
  onMacOs: true,
  seen: false,
  onboarded: true,
  onboardingShowing: false,
  launchDayCount: DOCK_NUDGE_AFTER_DAYS,
  pinState: { kind: 'offerable' } as DockPinState,
}

describe('shouldShowDockNudge', () => {
  it('offers the pin once the ledger has enough days', () => {
    expect(shouldShowDockNudge(ready)).toBe(true)
  })

  it('stays quiet on the launch before the threshold', () => {
    expect(shouldShowDockNudge({ ...ready, launchDayCount: DOCK_NUDGE_AFTER_DAYS - 1 })).toBe(false)
  })

  /**
   * "At least three days", ❌ not "the third day is today": the ledger started
   * counting when it shipped, and someone who has been here for months is
   * exactly who the offer is for.
   */
  it('offers the pin to a long-standing user, not only on the day it tips over', () => {
    expect(shouldShowDockNudge({ ...ready, launchDayCount: 400 })).toBe(true)
  })

  it('never asks twice, however long the ledger gets', () => {
    expect(shouldShowDockNudge({ ...ready, seen: true, launchDayCount: 400 })).toBe(false)
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

describe('dockNudgeCouldFire', () => {
  it('lets a settled Mac through, so the gate goes on to ask the backend', () => {
    expect(dockNudgeCouldFire(ready)).toBe(true)
  })

  it.each([
    ['an automated run', { automatedRun: true }],
    ['a machine with no Dock', { onMacOs: false }],
    ['an offer already made', { seen: true }],
    ['onboarding still unfinished', { onboarded: false }],
    ['the wizard on screen', { onboardingShowing: true }],
  ] as const)('turns around on %s, before any IPC is paid for', (_case, override) => {
    expect(dockNudgeCouldFire({ ...ready, ...override })).toBe(false)
  })
})
