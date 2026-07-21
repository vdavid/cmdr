/**
 * Component tests for `DriveIndexBadge.svelte`: the state→color mapping renders
 * the right class, the click menu shows the right items + footer per state, and
 * the scanning tooltip renders the shared status body from `index-state` live
 * activity (or the static fallback when there's no activity yet). The pure
 * mapping is covered in `drive-index-status.test.ts`; this verifies the component
 * honors it.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import type { ActivityPhase, VolumeIndexStatus } from '$lib/ipc/bindings'
import type { VolumeIndexActivity } from '$lib/indexing'

// The badge reads its own volume's live activity + phase from `index-state` (the
// single live-activity source). Mock it so we can drive the scanning tooltip body.
let badgeActivity: VolumeIndexActivity | undefined
let badgePhase: ActivityPhase | undefined
vi.mock('$lib/indexing', () => ({
  getVolumeActivity: () => badgeActivity,
  getVolumeAggregation: () => undefined,
  getVolumePhase: () => badgePhase,
  placeholderActivity: (volumeId: string): VolumeIndexActivity => ({
    volumeId,
    phase: 'scanning',
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    scanStartedAt: 0,
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
  }),
}))

import DriveIndexBadge from './DriveIndexBadge.svelte'

function scanActivity(overrides: Partial<VolumeIndexActivity> = {}): VolumeIndexActivity {
  return {
    volumeId: 'smb-test',
    phase: 'scanning',
    entriesScanned: 12_345,
    dirsFound: 678,
    bytesScanned: 1_000_000,
    scanStartedAt: Date.now() - 4000,
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
    ...overrides,
  }
}

/** Query an HTML element that must exist; fails the test loudly if it doesn't. */
function must(root: ParentNode, selector: string): HTMLElement {
  const el = root.querySelector<HTMLElement>(selector)
  if (!el) throw new Error(`expected element matching ${selector}`)
  return el
}

function makeStatus(overrides: Partial<VolumeIndexStatus> = {}): VolumeIndexStatus {
  return {
    volumeId: 'smb-test',
    enabled: true,
    freshness: 'fresh',
    failure: null,
    scanCompletedAt: 1_750_000_000,
    scanDurationMs: 134_000,
    coalescedSignalsSinceSweep: 0,
    nextSweepDueAt: null,
    ...overrides,
  }
}

function render(status: VolumeIndexStatus, onAction = vi.fn()) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DriveIndexBadge, {
    target,
    props: { volumeId: status.volumeId, status, driveName: 'Backups', onAction },
  })
  flushSync()
  return { target, onAction }
}

/** The badge's aria-label embeds the resolved tooltip text (`ariaLabel: tooltip`). */
function ariaLabel(target: HTMLElement): string {
  return must(target, '.drive-index-badge').getAttribute('aria-label') ?? ''
}

beforeEach(() => {
  badgeActivity = undefined
  badgePhase = undefined
})

describe('DriveIndexBadge color class', () => {
  it('renders the gray class when disabled', () => {
    const { target } = render(makeStatus({ enabled: false, freshness: null }))
    expect(target.querySelector('.drive-index-badge-disabled')).not.toBeNull()
  })

  it('renders the blue class while scanning', () => {
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    expect(target.querySelector('.drive-index-badge-scanning')).not.toBeNull()
  })

  it('renders the green class when fresh', () => {
    const { target } = render(makeStatus({ freshness: 'fresh' }))
    expect(target.querySelector('.drive-index-badge-fresh')).not.toBeNull()
  })

  it('renders the yellow class when stale', () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    expect(target.querySelector('.drive-index-badge-stale')).not.toBeNull()
  })
})

describe('DriveIndexBadge menu', () => {
  function openMenu(target: HTMLElement) {
    must(target, '.drive-index-badge').click()
    flushSync()
  }

  function menuLabels(target: HTMLElement): string[] {
    return [...target.querySelectorAll<HTMLElement>('.drive-index-menu-item')].map((el) => el.textContent.trim())
  }

  it('a disabled drive offers only "Turn on indexing for this drive"', () => {
    const { target } = render(makeStatus({ enabled: false, freshness: null }))
    openMenu(target)
    expect(menuLabels(target)).toEqual(['Turn on indexing for this drive'])
  })

  it('a scanning drive offers stop + forget', () => {
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    openMenu(target)
    expect(menuLabels(target)).toEqual(['Stop indexing', "Forget this drive's index"])
  })

  it('a fresh/stale drive offers rescan + turn off + forget', () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    openMenu(target)
    expect(menuLabels(target)).toEqual(['Rescan now', 'Turn off indexing for this drive', "Forget this drive's index"])
  })

  it('shows the last-indexed footer only when scan facts exist', () => {
    const withFacts = render(makeStatus({ freshness: 'fresh' }))
    openMenu(withFacts.target)
    expect(withFacts.target.querySelector('.drive-index-menu-footer')).not.toBeNull()

    const noFacts = render(makeStatus({ freshness: 'fresh', scanCompletedAt: null, scanDurationMs: null }))
    openMenu(noFacts.target)
    expect(noFacts.target.querySelector('.drive-index-menu-footer')).toBeNull()
  })

  it('calls onAction with the volume id and picked action', () => {
    const { target, onAction } = render(makeStatus({ freshness: 'stale' }))
    openMenu(target)
    must(target, '.drive-index-menu-item').click()
    flushSync()
    expect(onAction).toHaveBeenCalledWith('smb-test', 'rescan')
  })
})

describe('DriveIndexBadge scanning tooltip', () => {
  it('falls back to the static scanning phrasing when there is no live activity yet', () => {
    badgeActivity = undefined
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    // Unified onto the indexing.scan.* family; no rich body host rendered.
    expect(ariaLabel(target)).toContain('Scanning your drive')
    expect(target.querySelector('.scan-tooltip-body')).toBeNull()
  })

  it('renders the shared checklist body (count + elapsed) once live activity is present', () => {
    badgeActivity = scanActivity({ volumeUsedBytes: 10_000_000 }) // rough first scan: count + elapsed, no bar
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    const body = target.querySelector('.scan-tooltip-body')
    expect(body).not.toBeNull()
    // The checklist's first step (a network drive: Find files, then Compute folder sizes).
    expect(body?.textContent).toContain('Find files')
    expect(body?.textContent).toContain('12,345')
    // Rough first scan → no progress bar.
    expect(body?.querySelector('[role="progressbar"]')).toBeNull()
  })

  it('renders the calibrated bar when the scan has a prior-scan denominator', () => {
    badgeActivity = scanActivity({ priorTotalEntries: 100_000 })
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    const body = target.querySelector('.scan-tooltip-body')
    expect(body?.querySelector('[role="progressbar"]')).not.toBeNull()
  })

  it('renders the checklist from the phase alone when there is no live activity (reconcile / pre-tick)', () => {
    badgeActivity = undefined
    badgePhase = 'scanning'
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    // A phase but no activity yet: the body still renders (off a placeholder), so
    // the checklist stays visible instead of falling back to the static phrase.
    expect(target.querySelector('.scan-tooltip-body')).not.toBeNull()
    expect(target.querySelector('.scan-tooltip-body')?.textContent).toContain('Find files')
  })

  it('does not render the rich body when the badge is not scanning', () => {
    badgeActivity = scanActivity()
    const { target } = render(makeStatus({ freshness: 'fresh' }))
    expect(target.querySelector('.scan-tooltip-body')).toBeNull()
  })
})

describe('DriveIndexBadge coalesced-signal note', () => {
  /**
   * A status whose last full check was just under 24 h ago (the spans round UP,
   * so a hair over would read as 25), with the next one `inHours` away.
   */
  function sweptStatus(count: number, inHours: number | null): VolumeIndexStatus {
    const nowSeconds = Math.floor(Date.now() / 1000)
    return makeStatus({
      coalescedSignalsSinceSweep: count,
      scanCompletedAt: nowSeconds - (24 * 3600 - 60),
      nextSweepDueAt: inHours == null ? null : nowSeconds + inHours * 3600,
    })
  }

  it('says nothing extra when macOS never lost track', () => {
    const { target } = render(sweptStatus(0, 6))
    expect(ariaLabel(target)).not.toContain('lost track')
  })

  it('reads in the singular for a single skipped signal', () => {
    const { target } = render(sweptStatus(1, 6))
    expect(ariaLabel(target)).toContain(
      'macOS lost track of file system changes once in the last 24 hours, so a few folder sizes might be slightly off.',
    )
    expect(ariaLabel(target)).toContain("Cmdr's next full check in 6 hours will fix it.")
  })

  it('reads in the plural for several skipped signals', () => {
    expect(ariaLabel(render(sweptStatus(12, 6)).target)).toContain(
      'macOS lost track of file system changes 12 times in the last 24 hours',
    )
  })

  it('says "an hour", not "1 hours", when the next check is close', () => {
    expect(ariaLabel(render(sweptStatus(2, 1)).target)).toContain("Cmdr's next full check in an hour will fix it.")
  })

  it('drops the next-check promise for a drive with no scheduled sweep', () => {
    // An external drive keeps a 45-second debounce, which is no promise of a
    // future check, so the tooltip must not invent one.
    const label = ariaLabel(render(sweptStatus(3, null)).target)
    expect(label).toContain('macOS lost track of file system changes 3 times in the last 24 hours')
    expect(label).toContain("It's usually caches full of small files, so it's no big deal.")
    expect(label).not.toContain('next full check')
  })

  it('keeps the note out of the tooltip while a scan is running', () => {
    const { target } = render({ ...sweptStatus(4, 6), freshness: 'scanning' })
    expect(ariaLabel(target)).not.toContain('lost track')
  })
})
