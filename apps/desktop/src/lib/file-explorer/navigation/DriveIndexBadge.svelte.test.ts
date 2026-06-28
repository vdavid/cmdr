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
import type { VolumeIndexStatus } from '$lib/ipc/bindings'
import type { VolumeIndexActivity } from '$lib/indexing'

// The badge reads its own volume's live activity from `index-state` (the single
// live-activity source). Mock it so we can drive the scanning tooltip body.
let badgeActivity: VolumeIndexActivity | undefined
vi.mock('$lib/indexing', () => ({
  getVolumeActivity: () => badgeActivity,
  getVolumeAggregation: () => undefined,
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
    scanCompletedAt: 1_750_000_000,
    scanDurationMs: 134_000,
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

  it('renders the shared status body (count + elapsed) once live activity is present', () => {
    badgeActivity = scanActivity({ volumeUsedBytes: 10_000_000 }) // rough first scan: count + elapsed, no bar
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    const body = target.querySelector('.scan-tooltip-body')
    expect(body).not.toBeNull()
    expect(body?.textContent).toContain('Scanning your drive')
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

  it('does not render the rich body when the badge is not scanning', () => {
    badgeActivity = scanActivity()
    const { target } = render(makeStatus({ freshness: 'fresh' }))
    expect(target.querySelector('.scan-tooltip-body')).toBeNull()
  })
})
