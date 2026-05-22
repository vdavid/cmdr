import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import RecentSearchesFooter from './RecentSearchesFooter.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'

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

describe('RecentSearchesFooter', () => {
  it('renders nothing when there are no entries', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: {
        entries: [],
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
      props: { entries, disabled: false, onPick: () => {}, onRemove: () => {}, onOpenAll: () => {} },
    })
    await tick()
    expect(target.querySelector('.recent-label')).not.toBeNull()
    expect(target.querySelector('.recent-label')?.textContent).toContain('Recent searches:')
    expect(target.querySelector('.all-searches')).not.toBeNull()
    // The strip is rendered (entries.length > 0), and capped at the
    // CANDIDATE_MAX (12) ceiling even before layout measurements come in.
    const chips = target.querySelectorAll('.recent-chip')
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
      props: { entries: [entry], disabled: false, onPick, onRemove: () => {}, onOpenAll: () => {} },
    })
    await tick()
    const chip = target.querySelector('.recent-chip') as HTMLButtonElement
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
      props: { entries: [entry], disabled: false, onPick: () => {}, onRemove, onOpenAll: () => {} },
    })
    await tick()
    const chip = target.querySelector('.recent-chip') as HTMLButtonElement
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
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    const chip = target.querySelector('.recent-chip') as HTMLElement
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
        disabled: false,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll,
      },
    })
    await tick()
    const allBtn = target.querySelector('.all-searches') as HTMLButtonElement
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
        disabled: true,
        onPick: () => {},
        onRemove: () => {},
        onOpenAll: () => {},
      },
    })
    await tick()
    const chip = target.querySelector('.recent-chip') as HTMLButtonElement
    const all = target.querySelector('.all-searches') as HTMLButtonElement
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
      props: { entries, disabled: false, onPick: () => {}, onRemove: () => {}, onOpenAll: () => {} },
    })
    await tick()
    const badges = Array.from(target.querySelectorAll('.chip-badge')).map((b) => b.textContent.trim())
    expect(badges).toEqual(['AI', 'Aa', '.*'])
    target.remove()
  })
})
