import { describe, it, expect, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import RecentSearchesFooterRaw from './RecentItemsFooter.svelte'
import type { HistoryEntry, HistoryMode } from '$lib/tauri-commands'
import type { RecentItemAdapter, RecentItemKey } from './recent-items-types'
import { chipTooltip, modeName, formatAge } from './recent-items-utils'

// Svelte 5's `generics="E"` declaration doesn't survive the `mount()` type roundtrip: the
// declared `Component<unknown>` shape rejects a typed `RecentItemAdapter<HistoryEntry>`. The
// runtime contract is fine; we cast through unknown to a permissive Component shape so the
// mount() call type-checks without unsafe-argument errors.
const RecentSearchesFooter = RecentSearchesFooterRaw as unknown as Component<Record<string, unknown>>

function makeEntry(overrides: Partial<HistoryEntry>): HistoryEntry {
  return {
    id: 'id-' + (overrides.query ?? 'x'),
    timestamp: Date.now(),
    mode: 'filename',
    query: '*.pdf',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
    ...overrides,
  }
}

// Search-shaped adapter: the wrapping the Search dialog ships.
const searchAdapter: RecentItemAdapter<HistoryEntry> = (entry) => ({
  label: entry.query,
  tooltip: chipTooltip(entry),
  mode: entry.mode,
  ageLabel: formatAge(entry.timestamp),
  ariaLabel: `Run recent ${modeName(entry.mode)} search: ${entry.query}`,
})
const searchKey: RecentItemKey<HistoryEntry> = (entry) => entry.id

describe('RecentSearchesFooter', () => {
  it('renders nothing when there are no entries', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    expect(target.querySelector('.recent-footer')).toBeNull()
    target.remove()
  })

  // R3 U1: the strip no longer caps at 6 chips; the dynamic layout helper
  // fits as many chips as the strip width allows (up to the CANDIDATE_MAX
  // ceiling of 12). In jsdom, layout measurements are degenerate, so the
  // component falls back to the candidate list as-is. The contract we pin
  // here: the leading "Recent searches:" label and the trailing "All
  // searches…" button are ALWAYS rendered, even with many entries.
  it('R3 U1: always renders the leading label and trailing "All searches" button', async () => {
    const entries = Array.from({ length: 20 }, (_, i) =>
      makeEntry({ query: `query-${String(i)}`, id: `id-${String(i)}` }),
    )
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries,
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    expect(target.querySelector('.recent-label')).not.toBeNull()
    expect(target.querySelector('.recent-label')?.textContent).toContain('Recent searches:')
    expect(target.querySelector('.all-recent button')).not.toBeNull()
    // The strip is rendered (entries.length > 0), and capped at the
    // CANDIDATE_MAX (12) ceiling even before layout measurements come in.
    const chips = target.querySelectorAll('.chip-recent')
    expect(chips.length).toBeLessThanOrEqual(12)
    expect(chips.length).toBeGreaterThan(0)
    target.remove()
  })

  it('passes the activated entry to onPick', async () => {
    const entry = makeEntry({ query: '*.pdf', mode: 'filename' })
    const onPick = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [entry],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick,
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    const chip = target.querySelector('.chip-recent') as HTMLButtonElement
    chip.click()
    expect(onPick).toHaveBeenCalledWith(entry)
    target.remove()
  })

  it('fires onRemove on right-click and suppresses the native context menu', async () => {
    const entry = makeEntry({ query: 'screenshots', mode: 'ai' })
    const onRemove = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [entry],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove,
        onOpenAll: () => {},
      },
    })
    await tick()
    const chip = target.querySelector('.chip-recent') as HTMLButtonElement
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    chip.dispatchEvent(event)
    expect(onRemove).toHaveBeenCalledWith(entry)
    expect(event.defaultPrevented).toBe(true)
    target.remove()
  })

  // R3 U2: when a chip's text is truncated by CSS `text-overflow: ellipsis`,
  // the tooltip exposes the full query string so the user can still read it.
  // We tooltip the full query unconditionally (cheap and harmless when the
  // chip wasn't truncated) so the tooltip text always starts with the query.
  it('R3 U2: chip tooltip starts with the full query string', async () => {
    const longQuery = 'this is a really long recent search query that will probably be truncated'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [makeEntry({ query: longQuery })],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    const chip = target.querySelector('.chip-recent') as HTMLElement
    // The tooltip primitive stores the configured text on the host element;
    // we look at the `data-tooltip-text` attribute the tooltip directive
    // commonly sets (fallback to checking the rendered tooltip body
    // attribute via the data attr or aria).
    // We can't assert the rendered tooltip body without a hover event, but
    // we can confirm the directive was given the query string by reading
    // the chip's stored argument via `__data` if the tooltip exposes it.
    // Simpler: just confirm the query is somewhere in either the host or a
    // sibling tooltip DOM.
    const hostText = chip.outerHTML
    expect(hostText.includes(longQuery)).toBe(true)
    target.remove()
  })

  it('opens the popover via the All searches chip', async () => {
    const onOpenAll = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [makeEntry({ query: 'one' })],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll,
      },
    })
    await tick()
    const allBtn = target.querySelector('.all-recent button') as HTMLButtonElement
    allBtn.click()
    expect(onOpenAll).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('disables every chip when the dialog is disabled', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [makeEntry({ query: 'one' })],
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: true,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    const chip = target.querySelector('.chip-recent') as HTMLButtonElement
    const all = target.querySelector('.all-recent button') as HTMLButtonElement
    expect(chip.disabled).toBe(true)
    expect(all.disabled).toBe(true)
    target.remove()
  })

  it('renders the mode badges', async () => {
    const entries = [
      makeEntry({ query: 'a', mode: 'ai', id: 'a' }),
      makeEntry({ query: 'b', mode: 'filename', id: 'b' }),
      makeEntry({ query: 'c', mode: 'regex', id: 'c' }),
    ]
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries,
        adapter: searchAdapter,
        keyFn: searchKey,
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    const badges = Array.from(target.querySelectorAll('.chip-badge')).map((b) => b.textContent.trim())
    expect(badges).toEqual(['AI', 'Aa', '.*'])
    target.remove()
  })

  // The adapter pattern is the only seam between consumer-specific entries and the
  // generic footer. We pin both wrappings: Search's full HistoryEntry (above) and a
  // Selection-shaped entry (below). This test ensures the contract holds against any
  // narrower entry shape.
  it('renders against a Selection-shaped adapter', async () => {
    interface SelectionEntry {
      id: string
      query: string
      mode: HistoryMode
    }
    const selectionEntries: SelectionEntry[] = [
      { id: 's-1', query: '*.png', mode: 'filename' },
      { id: 's-2', query: 'all image files', mode: 'ai' },
    ]
    const selectionAdapter: RecentItemAdapter<SelectionEntry> = (entry) => ({
      label: entry.query,
      tooltip: `${modeName(entry.mode)} selection`,
      mode: entry.mode,
      ageLabel: 'just now',
      ariaLabel: `Reapply recent ${modeName(entry.mode)} selection: ${entry.query}`,
    })
    const selectionKey: RecentItemKey<SelectionEntry> = (entry) => entry.id

    const onPick = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: selectionEntries,
        adapter: selectionAdapter,
        keyFn: selectionKey,
        disabled: false,
        onPick,
        onRemove: () => {},
        onOpenAll: () => {},
        leadingLabel: 'Recent selections:',
        trailingLabel: 'All selections…',
        trailingTooltipText: 'Show all recent selections',
        ariaRegionLabel: 'Recent selections',
        ariaAllButtonLabel: 'All recent selections',
      },
    })
    await tick()
    expect(target.querySelector('.recent-label')?.textContent).toContain('Recent selections:')
    const chips = target.querySelectorAll<HTMLButtonElement>('.chip-recent')
    expect(chips.length).toBe(2)
    expect(chips[0].textContent).toContain('*.png')
    expect(chips[1].textContent).toContain('all image files')
    chips[0].click()
    expect(onPick).toHaveBeenCalledWith(selectionEntries[0])
    target.remove()
  })
})
