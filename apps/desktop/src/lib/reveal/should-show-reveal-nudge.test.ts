/**
 * When Cmdr may offer to catch "Show in Finder", and when it must stay quiet.
 *
 * The offer is once-ever and it changes a machine-wide macOS preference, so a
 * wrong fire is both unrecoverable and rude: the rows about the key's current
 * holder are the important half of this file.
 */

import { describe, it, expect } from 'vitest'
import type { RevealHandlerState } from '$lib/ipc/bindings'
import { emptyNudgeLedger } from '$lib/nudges/nudge-ledger'
import { REVEAL_NUDGE_AFTER_DAYS, shouldShowRevealNudge, type RevealNudgeInputs } from './should-show-reveal-nudge'

const NOW = new Date('2026-09-09T12:00:00Z')

/** `days` before {@link NOW}, as the ISO instant the nudge ledger stores. */
function daysAgo(days: number): string {
  return new Date(NOW.getTime() - days * 24 * 60 * 60 * 1000).toISOString()
}

/** A settled Mac on its second day, with nobody holding the key: the one case that speaks up. */
const ready: RevealNudgeInputs = {
  automatedRun: false,
  onMacOs: true,
  onboarded: true,
  onboardingShowing: false,
  ledger: emptyNudgeLedger(),
  now: NOW,
  launchDayCount: REVEAL_NUDGE_AFTER_DAYS,
  handlerState: { kind: 'notRegistered' } satisfies RevealHandlerState,
}

describe('shouldShowRevealNudge', () => {
  it('offers the handler once the ledger has enough days', () => {
    expect(shouldShowRevealNudge(ready)).toBe(true)
  })

  it('stays quiet on the launch before the threshold', () => {
    expect(shouldShowRevealNudge({ ...ready, launchDayCount: REVEAL_NUDGE_AFTER_DAYS - 1 })).toBe(false)
  })

  /**
   * "At least two days", ❌ not "the second day is today": the ledger started
   * counting when it shipped, and someone who has been here for months is
   * exactly who the offer is for.
   */
  it('offers the handler to a long-standing user, not only on the day it tips over', () => {
    expect(shouldShowRevealNudge({ ...ready, launchDayCount: 400 })).toBe(true)
  })

  it('never asks twice, however long the ledger gets', () => {
    const ledger = { dockPin: '', reveal: daysAgo(900) }
    expect(shouldShowRevealNudge({ ...ready, ledger, launchDayCount: 400 })).toBe(false)
  })

  it('waits its turn when another nudge spoke a day ago', () => {
    const ledger = { dockPin: daysAgo(1), reveal: '' }
    expect(shouldShowRevealNudge({ ...ready, ledger })).toBe(false)
  })

  it('says nothing when reveals already land here', () => {
    expect(shouldShowRevealNudge({ ...ready, handlerState: { kind: 'registered' } })).toBe(false)
  })

  /**
   * Someone running Path Finder or ForkLift chose that on purpose. Offering to
   * take a working setup over, unprompted, is rude; the Settings row is where a
   * take-over belongs, because there the person asked.
   */
  it('leaves another file manager alone rather than offering to take its key', () => {
    const handlerState: RevealHandlerState = {
      kind: 'heldByOtherApp',
      bundleId: 'com.cocoatech.PathFinder',
      displayName: 'Path Finder',
    }
    expect(shouldShowRevealNudge({ ...ready, handlerState })).toBe(false)
  })

  /**
   * `unavailable` is a build that must never write the key (debug, worktree,
   * E2E) as well as every non-macOS platform. Offering there would promise
   * something the click cannot deliver.
   */
  it('says nothing where the key cannot be written at all', () => {
    expect(shouldShowRevealNudge({ ...ready, handlerState: { kind: 'unavailable' } })).toBe(false)
  })

  it('stays quiet under an automated run, so it cannot leak into the first spec', () => {
    expect(shouldShowRevealNudge({ ...ready, automatedRun: true })).toBe(false)
  })

  it('stays quiet off macOS, where there is no NSFileViewer to hold', () => {
    expect(shouldShowRevealNudge({ ...ready, onMacOs: false })).toBe(false)
  })

  it('waits for onboarding to be finished', () => {
    expect(shouldShowRevealNudge({ ...ready, onboarded: false })).toBe(false)
  })

  it('waits for the wizard to leave the screen', () => {
    expect(shouldShowRevealNudge({ ...ready, onboardingShowing: true })).toBe(false)
  })
})

describe('the two thresholds together', () => {
  /**
   * The reveal offer is the more valuable of the two, so it goes first: on a
   * daily user's second launch day, only the reveal offer is eligible.
   */
  it('reaches the reveal offer a launch day before the Dock offer could fire', async () => {
    const { DOCK_NUDGE_AFTER_DAYS } = await import('$lib/dock/should-show-dock-nudge')
    expect(REVEAL_NUDGE_AFTER_DAYS).toBeLessThan(DOCK_NUDGE_AFTER_DAYS)
  })
})
