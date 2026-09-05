import { describe, it, expect } from 'vitest'
import { classifyLanding, waitForPaneToGoQuiet, type PaneQuietProbe } from './mcp-nav-landing'

/**
 * A scripted pane: each `sleep` advances one step through the script and moves the
 * virtual clock, so the wait's timing rules are exercised without a real clock.
 * The last step repeats forever, which is what "the pane came to rest here" means.
 */
function scriptedProbe(steps: Array<{ listingId: string | null; loading: boolean }>): PaneQuietProbe & {
  elapsed: () => number
} {
  let index = 0
  let clock = 0
  const at = () => steps[Math.min(index, steps.length - 1)]
  return {
    getListingId: () => at().listingId,
    isLoading: () => at().loading,
    now: () => clock,
    sleep: (ms: number) => {
      clock += ms
      index += 1
      return Promise.resolve()
    },
    elapsed: () => clock,
  }
}

const WAIT = { listingIdBefore: 'before', budgetMs: 2_000, pollMs: 100, quietMs: 250 }

describe('waitForPaneToGoQuiet', () => {
  it('reports quiet once a new listing has started and the pane has been idle for quietMs', async () => {
    const probe = scriptedProbe([
      { listingId: 'before', loading: true },
      { listingId: 'after', loading: true },
      { listingId: 'after', loading: false },
      { listingId: 'after', loading: false },
      { listingId: 'after', loading: false },
    ])

    await expect(waitForPaneToGoQuiet(probe, WAIT)).resolves.toBe(true)
  })

  it('does NOT call it quiet while the switch is still listing', async () => {
    // The switch arm commits the destination optimistically and resolves `settled`
    // straight away, so an idle-looking pane whose listing hasn't started yet is
    // exactly the false-OK window this wait exists to close.
    const probe = scriptedProbe([
      { listingId: 'before', loading: false },
      { listingId: 'before', loading: false },
      { listingId: 'after', loading: true },
      { listingId: 'after', loading: false },
      { listingId: 'after', loading: false },
    ])

    await expect(waitForPaneToGoQuiet(probe, WAIT)).resolves.toBe(true)
    // Quiet is only declared after the new listing settles, not on the idle
    // observations that precede it.
    expect(probe.elapsed()).toBeGreaterThanOrEqual(400)
  })

  it('re-arms the quiet window when a second listing starts, so a fallback is not read as success', async () => {
    // The failing listing settles, the edge-flow fallback starts its own listing,
    // and only when THAT one comes to rest is the pane's location worth reading.
    const probe = scriptedProbe([
      { listingId: 'failed', loading: false },
      { listingId: 'failed', loading: false },
      { listingId: 'fallback', loading: true },
      { listingId: 'fallback', loading: true },
      { listingId: 'fallback', loading: false },
      { listingId: 'fallback', loading: false },
      { listingId: 'fallback', loading: false },
    ])

    await expect(waitForPaneToGoQuiet(probe, WAIT)).resolves.toBe(true)
    expect(probe.elapsed()).toBeGreaterThanOrEqual(600)
  })

  it('gives up when no listing ever starts within the budget', async () => {
    const probe = scriptedProbe([{ listingId: 'before', loading: false }])

    await expect(waitForPaneToGoQuiet(probe, WAIT)).resolves.toBe(false)
    expect(probe.elapsed()).toBeGreaterThanOrEqual(WAIT.budgetMs)
  })

  it('treats a pane that never had a listing as quiet once it gets one', async () => {
    const probe = scriptedProbe([
      { listingId: null, loading: true },
      { listingId: 'first', loading: false },
      { listingId: 'first', loading: false },
      { listingId: 'first', loading: false },
    ])

    await expect(waitForPaneToGoQuiet(probe, { ...WAIT, listingIdBefore: null })).resolves.toBe(true)
  })
})

describe('classifyLanding', () => {
  const target = { volumeId: 'root', path: '/Users/david/code' }

  it('calls it navigated when the pane came to rest on the target', () => {
    expect(classifyLanding({ target, landed: { volumeId: 'root', path: '/Users/david/code' }, quiet: true })).toEqual({
      outcome: 'navigated',
      volumeId: 'root',
      path: '/Users/david/code',
    })
  })

  it('ignores a trailing slash on either side', () => {
    expect(classifyLanding({ target, landed: { volumeId: 'root', path: '/Users/david/code/' }, quiet: true })).toEqual({
      outcome: 'navigated',
      volumeId: 'root',
      path: '/Users/david/code/',
    })
  })

  it('calls it a fallback when the pane came to rest somewhere else', () => {
    expect(classifyLanding({ target, landed: { volumeId: 'root', path: '/Users/david' }, quiet: true })).toEqual({
      outcome: 'fell-back',
      volumeId: 'root',
      path: '/Users/david',
    })
  })

  it('calls it a fallback when the pane is on another volume, same-looking path or not', () => {
    expect(classifyLanding({ target, landed: { volumeId: 'mtp-1', path: '/Users/david/code' }, quiet: true })).toEqual({
      outcome: 'fell-back',
      volumeId: 'mtp-1',
      path: '/Users/david/code',
    })
  })

  it('says so when the pane never settled, even though it holds the target optimistically', () => {
    expect(classifyLanding({ target, landed: { volumeId: 'root', path: '/Users/david/code' }, quiet: false })).toEqual({
      outcome: 'did-not-settle',
      volumeId: 'root',
      path: '/Users/david/code',
    })
  })
})
