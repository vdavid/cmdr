/**
 * The `update_check` vocabulary. These props are written once and read off a dashboard months
 * later, so what they can and can't carry is the test.
 */

import { describe, expect, it, vi } from 'vitest'

// Pure vocabulary only: the IPC wrapper is stubbed so importing this module doesn't pull in
// `$lib/ipc/bindings` and, through it, the whole Tauri surface.
vi.mock('$lib/tauri-commands', () => ({ trackEvent: vi.fn() }))

import { blockerFailure, updateCheckProps, type UpdateCheckOutcome, type UpdateCheckTrigger } from './update-analytics'

describe('updateCheckProps', () => {
  it('reports a clean check with no failure and no staged build', () => {
    expect(updateCheckProps({ trigger: 'poll', outcome: 'up_to_date' })).toEqual({
      trigger: 'poll',
      outcome: 'up_to_date',
      failure: 'none',
      staged_version: 'none',
    })
  })

  it('keeps every entry point in its own bucket', () => {
    // The whole point of `trigger`: a run of manual checks (someone hunting for a fix) and the
    // background loop ticking are otherwise the same number.
    const triggers: UpdateCheckTrigger[] = ['startup', 'poll', 'auto_check_on', 'command', 'settings']
    const reported = triggers.map((trigger) => updateCheckProps({ trigger, outcome: 'up_to_date' }).trigger)
    expect(new Set(reported).size).toBe(triggers.length)
    expect(reported).toEqual(triggers)
  })

  it('names the release a check just staged', () => {
    expect(updateCheckProps({ trigger: 'poll', outcome: 'staged', stagedVersion: '0.33.0' })).toMatchObject({
      outcome: 'staged',
      staged_version: '0.33.0',
    })
  })

  it('separates a build already waiting for a restart from an up-to-date install', () => {
    // The whole point of `already_staged`: a rising count of it against a flat `staged` is the
    // population running an old build with a newer one downloaded, which is otherwise invisible.
    const stuck = updateCheckProps({ trigger: 'poll', outcome: 'already_staged', stagedVersion: '0.29.0' })
    const current = updateCheckProps({ trigger: 'poll', outcome: 'up_to_date' })
    expect(stuck.outcome).not.toBe(current.outcome)
    expect(stuck.staged_version).toBe('0.29.0')
  })

  it('keeps the three failing phases apart', () => {
    const phases = (['check', 'download', 'install'] as const).map(
      (failure) => updateCheckProps({ trigger: 'poll', outcome: 'failed', failure }).failure,
    )
    expect(new Set(phases).size).toBe(3)
  })

  it('carries a blocked install as its own outcome, with the arrangement as the failure', () => {
    expect(updateCheckProps({ trigger: 'poll', outcome: 'blocked', failure: 'translocated' })).toMatchObject({
      outcome: 'blocked',
      failure: 'translocated',
    })
  })

  /**
   * The debug-build net in `posthog::sanitize_props` only warns, so the vocabulary has to be right
   * here. Nothing on this event may look like a URL, a path, or a sentence.
   */
  it('emits only short categorical tokens', () => {
    const reports: {
      trigger: UpdateCheckTrigger
      outcome: UpdateCheckOutcome
      failure?: 'read_only_volume'
      stagedVersion?: string
    }[] = [
      { trigger: 'startup', outcome: 'up_to_date' },
      { trigger: 'poll', outcome: 'staged', stagedVersion: '0.33.0' },
      { trigger: 'auto_check_on', outcome: 'already_staged', stagedVersion: '0.29.0' },
      { trigger: 'command', outcome: 'blocked', failure: 'read_only_volume' },
      { trigger: 'settings', outcome: 'failed' },
    ]
    for (const report of reports) {
      for (const [key, value] of Object.entries(updateCheckProps(report))) {
        expect(value, `prop '${key}' carries a non-categorical value: ${value}`).toMatch(/^[a-z0-9_.]{1,24}$/)
      }
    }
  })
})

describe('blockerFailure', () => {
  it('maps each IPC blocker to its own snake_case token', () => {
    expect(blockerFailure('translocated')).toBe('translocated')
    expect(blockerFailure('readOnlyVolume')).toBe('read_only_volume')
  })
})
