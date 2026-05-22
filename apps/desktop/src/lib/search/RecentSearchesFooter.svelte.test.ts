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

  it('caps the visible chips at 6 and always includes the All searches affordance', async () => {
    const entries = Array.from({ length: 10 }, (_, i) =>
      makeEntry({ query: `query-${String(i)}`, id: `id-${String(i)}` }),
    )
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesFooter, {
      target,
      props: { entries, disabled: false, onPick: () => {}, onRemove: () => {}, onOpenAll: () => {} },
    })
    await tick()
    const chips = target.querySelectorAll('.recent-chip')
    expect(chips).toHaveLength(6)
    expect(target.querySelector('.all-searches')).not.toBeNull()
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
    const badges = Array.from(target.querySelectorAll('.chip-badge')).map((b) => b.textContent?.trim() ?? '')
    expect(badges).toEqual(['AI', 'Aa', '.*'])
    target.remove()
  })
})
