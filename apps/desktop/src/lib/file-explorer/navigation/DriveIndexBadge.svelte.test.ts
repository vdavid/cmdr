/**
 * Component tests for `DriveIndexBadge.svelte`: the state→color mapping renders
 * the right class, the click menu shows the right items + footer per state, and
 * the scanning tooltip renders the shared status body from `index-state` live
 * activity (or the static fallback when there's no activity yet). The pure
 * mapping is covered in `drive-index-status.test.ts`; this verifies the component
 * honors it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, tick } from 'svelte'
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

// The badge reads the MASTER drive-indexing switch to decide whether this drive's
// own controls are overridden. Mock it so both sides of the gate are testable.
let masterIndexingEnabled = true
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
  getDriveIndexingEnabled: () => masterIndexingEnabled,
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
    unreadableLocations: 0,
    unreadableRetried: false,
    nextSweepDueAt: null,
    liveWatch: true,
    ...overrides,
  }
}

/** Torn down after each test: a portaled menu outlives the target it was mounted into. */
let mounted: (() => void)[] = []

function render(status: VolumeIndexStatus, onAction = vi.fn(), host?: HTMLElement) {
  const target = document.createElement('div')
  ;(host ?? document.body).appendChild(target)
  const component = mount(DriveIndexBadge, {
    target,
    props: { volumeId: status.volumeId, status, driveName: 'Backups', onAction },
  })
  mounted.push(() => void unmount(component))
  flushSync()
  return { target, onAction }
}

// ── The menu's selectors, in ONE place ────────────────────────────────────
// Every query starts from the document rather than the mount target, so a portaled
// surface is found the same way an inline one is. Only these helpers know the menu's
// shape; every assertion below is written against behavior.

function badge(target: HTMLElement): HTMLButtonElement {
  return must(target, '.drive-index-badge') as HTMLButtonElement
}

/**
 * Clicks the badge and waits for the surface to exist. ❗ Two ticks, ❌ not `flushSync`: the
 * menu portals to the body through Ark's `Portal`, which mounts its children inside a
 * `tick().then(…)` of its own, so the surface isn't there on the synchronous pass.
 */
async function openMenu(target: HTMLElement): Promise<void> {
  badge(target).click()
  await settle()
}

async function settle(): Promise<void> {
  flushSync()
  await tick()
  await tick()
}

function menuEl(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[data-menu]')
}

function menuLabels(): string[] {
  return [...document.querySelectorAll<HTMLElement>('[data-menu] [data-menu-row]')].map((el) => el.textContent.trim())
}

/** One action row, by the label a reader would click. */
function menuRow(label: string): HTMLElement {
  const row = [...document.querySelectorAll<HTMLElement>('[data-menu] [data-menu-row]')].find(
    (el) => el.textContent.trim() === label,
  )
  if (!row) throw new Error(`no menu row labelled ${label}`)
  return row
}

function noteEl(): HTMLElement | null {
  return document.querySelector<HTMLElement>('.drive-index-menu-note')
}

function footerEl(): HTMLElement | null {
  return document.querySelector<HTMLElement>('.drive-index-menu-footer')
}

/** A real click somewhere else: the pointer-down decides, and the click follows it. */
async function clickOutside(): Promise<void> {
  document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
  document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await settle()
}

async function pressKey(key: string): Promise<void> {
  document.body.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
  await settle()
}

/** The badge's aria-label embeds the resolved tooltip text (`ariaLabel: tooltip`). */
function ariaLabel(target: HTMLElement): string {
  return must(target, '.drive-index-badge').getAttribute('aria-label') ?? ''
}

beforeEach(() => {
  badgeActivity = undefined
  badgePhase = undefined
  masterIndexingEnabled = true
})

afterEach(() => {
  for (const dispose of mounted) dispose()
  mounted = []
  document.body.innerHTML = ''
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

  // Coalescing change signals into one full check a day is the DESIGNED operating
  // state, not a fault, so it must never raise a fault colour. Yellow stays
  // reserved for a check that failed to happen when it was due. At the rate a
  // busy machine produces these, a badge that went yellow would sit yellow all
  // day and train people to ignore it.
  it('stays green while change signals are being coalesced', () => {
    const { target } = render(makeStatus({ freshness: 'fresh', coalescedSignalsSinceSweep: 1_284 }))
    expect(target.querySelector('.drive-index-badge-fresh')).not.toBeNull()
    expect(target.querySelector('.drive-index-badge-stale')).toBeNull()
  })
})

describe('DriveIndexBadge menu', () => {
  it('a disabled drive offers only "Turn on indexing for this drive"', async () => {
    const { target } = render(makeStatus({ enabled: false, freshness: null }))
    await openMenu(target)
    expect(menuLabels()).toEqual(['Turn on indexing for this drive'])
  })

  it('a scanning drive offers stop + forget', async () => {
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    await openMenu(target)
    expect(menuLabels()).toEqual(['Stop indexing', 'Forget this drive’s index'])
  })

  it('a fresh/stale drive offers rescan + turn off + forget', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    expect(menuLabels()).toEqual(['Rescan now', 'Turn off indexing for this drive', 'Forget this drive’s index'])
  })

  it('shows the last-indexed footer when scan facts exist', async () => {
    const { target } = render(makeStatus({ freshness: 'fresh' }))
    await openMenu(target)
    expect(footerEl()).not.toBeNull()
  })

  it('shows no footer for a drive that has never been scanned', async () => {
    const { target } = render(makeStatus({ freshness: 'fresh', scanCompletedAt: null, scanDurationMs: null }))
    await openMenu(target)
    expect(footerEl()).toBeNull()
  })

  it('calls onAction with the volume id and picked action', async () => {
    const { target, onAction } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    menuRow('Rescan now').click()
    flushSync()
    expect(onAction).toHaveBeenCalledWith('smb-test', 'rescan')
  })

  it('closes on a pick', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    menuRow('Rescan now').click()
    flushSync()
    expect(menuEl()).toBeNull()
  })

  it('swaps every action for one explanation while drive indexing is off in Settings', async () => {
    // A drive whose own choice is "indexed and fresh" still can't act while the
    // master switch is off, so the menu says why instead of offering buttons the
    // backend would refuse.
    masterIndexingEnabled = false
    const { target } = render(makeStatus({ freshness: 'fresh' }))
    await openMenu(target)
    expect(menuLabels()).toEqual([])
    expect(noteEl()?.textContent).toContain('Drive indexing is off in Settings')
  })

  it('says the master switch is off in the tooltip, not "off for this drive"', () => {
    masterIndexingEnabled = false
    const { target } = render(makeStatus({ enabled: false, freshness: null }))
    expect(ariaLabel(target)).toContain('Drive indexing is off in Settings')
  })
})

/**
 * The menu SHELL: opening, closing, and who holds focus afterwards. Pinned before the menu
 * moved onto the house `Menu` primitive, so "the port changed nothing here" is provable
 * rather than asserted. Each one is written against behavior, so the port had to move only
 * the selector helpers above.
 */
describe('DriveIndexBadge menu shell', () => {
  it('opens on a click, and says so for assistive tech', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    expect(badge(target).getAttribute('aria-expanded')).toBe('false')
    await openMenu(target)
    expect(menuEl()).not.toBeNull()
    expect(badge(target).getAttribute('aria-expanded')).toBe('true')
  })

  it('closes again on a second click on the badge', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    await openMenu(target)
    expect(menuEl()).toBeNull()
    expect(badge(target).getAttribute('aria-expanded')).toBe('false')
  })

  it('closes on Escape and hands focus back', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    badge(target).focus()
    await openMenu(target)
    await pressKey('Escape')
    expect(menuEl()).toBeNull()
    expect(document.activeElement).toBe(badge(target))
  })

  it('closes on a click somewhere else', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    await clickOutside()
    expect(menuEl()).toBeNull()
  })

  /**
   * In a switcher row the badge opens a menu from inside one. The house `Menu` keeps the
   * outer one open for it, off the host it names here (`$lib/ui/DETAILS.md` § Menu); the
   * primitive's own suite pins the outer half. Before the port this held because the menu
   * was a CHILD of the row, which is also what the switcher's scroller clipped.
   */
  it('names the menu it was opened from inside of', async () => {
    const switcherSurface = document.createElement('div')
    switcherSurface.setAttribute('data-menu', '')
    switcherSurface.setAttribute('data-menu-instance', 'menu-switcher')
    document.body.appendChild(switcherSurface)

    const { target } = render(makeStatus({ freshness: 'stale' }), vi.fn(), switcherSurface)
    await openMenu(target)
    // By name, not `menuEl()`: the stand-in switcher above is a `[data-menu]` too.
    const own = document.querySelector('[data-menu][aria-label="Drive index status"]')
    expect(own?.getAttribute('data-menu-nested-in')).toBe('menu-switcher')
  })

  it('names no host in the breadcrumb, where it hangs off the chip', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    expect(menuEl()?.hasAttribute('data-menu-nested-in')).toBe(false)
  })

  it('stays open for a click on the menu itself', async () => {
    const { target } = render(makeStatus({ freshness: 'stale' }))
    await openMenu(target)
    const menu = menuEl()
    menu?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    menu?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    flushSync()
    expect(menuEl()).not.toBeNull()
  })
})

describe('DriveIndexBadge scanning tooltip', () => {
  /** Hover the dot: the rich body mounts only while its tooltip is open. */
  function openScanTooltip(target: HTMLElement): void {
    // A real pointer move first: an earlier test's keypress leaves the shared tooltip hover-suppressed.
    document.dispatchEvent(new MouseEvent('mousemove'))
    must(target, '.drive-index-badge').dispatchEvent(new MouseEvent('mouseenter'))
    flushSync()
  }

  it('keeps the rich body unmounted until the tooltip opens', () => {
    // Mounted, the body re-renders on every progress event and runs a 1 Hz clock.
    badgeActivity = scanActivity()
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    expect(target.querySelector('.scan-tooltip-body')).toBeNull()

    openScanTooltip(target)
    expect(target.querySelector('.scan-tooltip-body')).not.toBeNull()

    must(target, '.drive-index-badge').dispatchEvent(new MouseEvent('mouseleave'))
    flushSync()
    expect(target.querySelector('.scan-tooltip-body')).toBeNull()
  })

  it('falls back to the static scanning phrasing when there is no live activity yet', () => {
    badgeActivity = undefined
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    // Unified onto the indexing.scan.* family; no rich body host rendered.
    expect(ariaLabel(target)).toContain('Indexing your drive')
    expect(target.querySelector('.scan-tooltip-body')).toBeNull()
  })

  it('renders the shared checklist body (count + elapsed) once live activity is present', () => {
    badgeActivity = scanActivity({ volumeUsedBytes: 10_000_000 }) // rough first scan: count + elapsed, no bar
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    openScanTooltip(target)
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
    openScanTooltip(target)
    const body = target.querySelector('.scan-tooltip-body')
    expect(body?.querySelector('[role="progressbar"]')).not.toBeNull()
  })

  it('renders the checklist from the phase alone when there is no live activity (reconcile / pre-tick)', () => {
    badgeActivity = undefined
    badgePhase = 'scanning'
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    openScanTooltip(target)
    // A phase but no activity yet: the body still renders (off a placeholder), so
    // the checklist stays visible instead of falling back to the static phrase.
    expect(target.querySelector('.scan-tooltip-body')).not.toBeNull()
    expect(target.querySelector('.scan-tooltip-body')?.textContent).toContain('Find files')
  })

  it('does not render the rich body when the badge is not scanning', () => {
    badgeActivity = scanActivity()
    const { target } = render(makeStatus({ freshness: 'fresh' }))
    openScanTooltip(target)
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
    expect(ariaLabel(target)).toContain('Cmdr’s next full check in 6 hours will fix it.')
  })

  it('reads in the plural for several skipped signals', () => {
    expect(ariaLabel(render(sweptStatus(12, 6)).target)).toContain(
      'macOS lost track of file system changes 12 times in the last 24 hours',
    )
  })

  it('says "an hour", not "1 hours", when the next check is close', () => {
    expect(ariaLabel(render(sweptStatus(2, 1)).target)).toContain('Cmdr’s next full check in an hour will fix it.')
  })

  it('drops the next-check promise for a drive with no scheduled sweep', () => {
    // An external drive keeps a 45-second debounce, which is no promise of a
    // future check, so the tooltip must not invent one.
    const label = ariaLabel(render(sweptStatus(3, null)).target)
    expect(label).toContain('macOS lost track of file system changes 3 times in the last 24 hours')
    expect(label).toContain('It’s usually caches full of small files, so it’s no big deal.')
    expect(label).not.toContain('next full check')
  })

  it('points at the check in flight while one is running, instead of promising a later one', () => {
    const { target } = render({ ...sweptStatus(4, 6), freshness: 'scanning', scanCompletedAt: null })
    const label = ariaLabel(target)
    expect(label).toContain('macOS lost track of file system changes 4 times')
    expect(label).toContain('the check running right now will put them right.')
    // The promise of a LATER check would be stale while one is in flight, and the
    // running scan cleared the marker the "in the last N hours" window reads.
    expect(label).not.toContain('next full check')
    expect(label).not.toContain('in the last')
  })
})

describe('DriveIndexBadge stale tooltip', () => {
  it('says a drive may have changed while it was disconnected', () => {
    const label = ariaLabel(render(makeStatus({ freshness: 'stale' })).target)
    expect(label).toContain('may have changed while it was disconnected')
  })

  it('says a phone’s own changes show up after a rescan, since nothing watches it', () => {
    const label = ariaLabel(render(makeStatus({ freshness: 'stale', liveWatch: false })).target)
    expect(label).toContain('Changes made on the phone itself show up after a rescan')
    expect(label).not.toContain('disconnected')
  })
})

describe('DriveIndexBadge "done, with holes" footnote', () => {
  it('says nothing extra when a finished index read everything', () => {
    const label = ariaLabel(render(makeStatus({ freshness: 'fresh' })).target)
    expect(label).not.toContain('couldn’t read')
  })

  it('counts places, in the plural, with thousands separators', () => {
    // The number that made this worth doing: 1,497 marked directories on a real
    // machine are the places a reader would recognize, not the folder count.
    const label = ariaLabel(
      render(makeStatus({ freshness: 'fresh', unreadableLocations: 1497, unreadableRetried: false })).target,
    )
    expect(label).toContain('Cmdr couldn’t read 1,497 spots on this drive')
    expect(label).toContain('a search here may come back a little short')
  })

  it('reads in the singular for one place, and says Cmdr comes back to it', () => {
    const label = ariaLabel(
      render(makeStatus({ freshness: 'fresh', unreadableLocations: 1, unreadableRetried: true })).target,
    )
    expect(label).toContain('Cmdr couldn’t read one spot on this drive')
    expect(label).toContain('It comes back to them on its own.')
  })

  it('promises no retry for ground nothing will come back to', () => {
    const label = ariaLabel(
      render(makeStatus({ freshness: 'fresh', unreadableLocations: 2, unreadableRetried: false })).target,
    )
    expect(label).not.toContain('comes back to them on its own')
  })

  it('sits alongside the coalesced-signal note when both are true', () => {
    const nowSeconds = Math.floor(Date.now() / 1000)
    const label = ariaLabel(
      render(
        makeStatus({
          freshness: 'fresh',
          coalescedSignalsSinceSweep: 2,
          scanCompletedAt: nowSeconds - (24 * 3600 - 60),
          nextSweepDueAt: nowSeconds + 6 * 3600,
          unreadableLocations: 4,
          unreadableRetried: true,
        }),
      ).target,
    )
    expect(label).toContain('macOS lost track of file system changes')
    expect(label).toContain('Cmdr couldn’t read 4 spots on this drive')
  })
})
