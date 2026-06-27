/**
 * Component tests for `DriveIndexBadge.svelte`: the state→color mapping renders
 * the right class, and the click menu shows the right items + footer per state.
 * The pure mapping is covered separately in `drive-index-status.test.ts`; this
 * verifies the component honors it.
 */
import { describe, it, expect, vi } from 'vitest'
import { mount, flushSync } from 'svelte'
import DriveIndexBadge from './DriveIndexBadge.svelte'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'

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

function render(
  status: VolumeIndexStatus,
  onAction = vi.fn(),
  scanProgress?: { entriesScanned: number; scanStartedAt: number },
) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DriveIndexBadge, { target, props: { volumeId: status.volumeId, status, scanProgress, onAction } })
  flushSync()
  return { target, onAction }
}

/** The badge's aria-label embeds the resolved tooltip text (`ariaLabel: tooltip`). */
function ariaLabel(target: HTMLElement): string {
  return must(target, '.drive-index-badge').getAttribute('aria-label') ?? ''
}

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
  it('falls back to the static scanning phrasing before any progress arrives', () => {
    const { target } = render(makeStatus({ freshness: 'scanning' }))
    expect(ariaLabel(target)).toContain('Indexing this drive')
  })

  it('shows the live file count once progress has arrived', () => {
    const { target } = render(makeStatus({ freshness: 'scanning' }), vi.fn(), {
      entriesScanned: 12_345,
      scanStartedAt: Date.now(),
    })
    const label = ariaLabel(target)
    expect(label).toContain('Indexing…')
    // Thousands-separated count (separator is locale-dependent; check both ends).
    expect(label).toMatch(/12.345/)
    expect(label).toContain('files')
  })

  it('appends the elapsed clock once the scan has run for over a second', () => {
    const { target } = render(makeStatus({ freshness: 'scanning' }), vi.fn(), {
      entriesScanned: 7,
      scanStartedAt: Date.now() - 42_000,
    })
    expect(ariaLabel(target)).toContain('0:42')
  })

  it('ignores scan progress when the badge is not scanning', () => {
    const { target } = render(makeStatus({ freshness: 'fresh' }), vi.fn(), {
      entriesScanned: 999,
      scanStartedAt: Date.now(),
    })
    const label = ariaLabel(target)
    expect(label).not.toContain('Indexing…')
    expect(label).not.toContain('999')
  })
})
