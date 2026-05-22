import { describe, it, expect, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import RecentSearchesPopoverRaw from './RecentItemsPopover.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'
import type { RecentItemAdapter, RecentItemKey } from './recent-items-types'
import { chipTooltip, modeName, formatAge } from './recent-items-utils'

// Svelte 5 generics+mount type roundtrip workaround — see `RecentItemsFooter.svelte.test.ts`.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const RecentSearchesPopover = RecentSearchesPopoverRaw as any

function makeEntry(overrides: Partial<HistoryEntry>): HistoryEntry {
  return {
    id: 'id-' + (overrides.query ?? 'x'),
    timestamp: Date.now(),
    mode: 'filename',
    query: 'sample',
    filters: {},
    scope: '',
    caseSensitive: false,
    excludeSystemDirs: true,
    resultCount: 0,
    ...overrides,
  }
}

const searchAdapter: RecentItemAdapter<HistoryEntry> = (entry) => ({
  label: entry.query,
  tooltip: chipTooltip(entry),
  mode: entry.mode,
  ageLabel: formatAge(entry.timestamp),
  ariaLabel: `Run recent ${modeName(entry.mode)} search: ${entry.query}`,
})
const searchKey: RecentItemKey<HistoryEntry> = (entry) => entry.id

function setupAnchor(): HTMLButtonElement {
  const anchor = document.createElement('button')
  anchor.textContent = 'anchor'
  document.body.appendChild(anchor)
  return anchor
}

describe('RecentSearchesPopover', () => {
  it('does not render when open is false', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: false,
        entries: [makeEntry({ query: 'one' })],
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick: () => {},
        onRemove: () => {},
      },
    })
    await tick()
    expect(document.querySelector('.recent-popover')).toBeNull()
    target.remove()
    anchor.remove()
  })

  it('lists every entry on open with an empty filter input', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const entries = [
      makeEntry({ query: 'alpha', id: 'a', mode: 'filename' }),
      makeEntry({ query: 'beta', id: 'b', mode: 'ai' }),
      makeEntry({ query: 'gamma', id: 'c', mode: 'regex' }),
    ]
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries,
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick: () => {},
        onRemove: () => {},
      },
    })
    await tick()
    const rows = document.querySelectorAll('.result-row')
    expect(rows).toHaveLength(3)
    target.remove()
    anchor.remove()
  })

  it('filters entries fuzzily against query + mode badge', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const entries = [
      makeEntry({ query: 'screenshots', id: 's', mode: 'ai' }),
      makeEntry({ query: '*.pdf', id: 'p', mode: 'filename' }),
      makeEntry({ query: '*.dmg', id: 'd', mode: 'filename' }),
    ]
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries,
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick: () => {},
        onRemove: () => {},
      },
    })
    await tick()

    const input = document.querySelector('.search-field') as HTMLInputElement
    input.value = 'pdf'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    const rows = document.querySelectorAll('.result-row')
    expect(rows.length).toBeGreaterThanOrEqual(1)
    const queries = Array.from(rows).map((r) => r.textContent)
    expect(queries.some((q) => q.includes('*.pdf'))).toBe(true)

    target.remove()
    anchor.remove()
  })

  it('shows the empty message when no entry matches the filter', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries: [makeEntry({ query: 'screenshots' })],
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick: () => {},
        onRemove: () => {},
      },
    })
    await tick()
    const input = document.querySelector('.search-field') as HTMLInputElement
    input.value = 'zzzzzz'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(document.querySelector('.empty')?.textContent ?? '').toContain('No recent searches')
    target.remove()
    anchor.remove()
  })

  it('activates the cursor row on Enter', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const onPick = vi.fn()
    const entry = makeEntry({ query: 'first', id: 'f' })
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries: [entry, makeEntry({ query: 'second', id: 's' })],
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick,
        onRemove: () => {},
      },
    })
    await tick()
    const popover = document.querySelector('.recent-popover') as HTMLElement
    popover.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onPick).toHaveBeenCalledWith(entry)
    target.remove()
    anchor.remove()
  })

  it('right-click on a row triggers onRemove and suppresses the native menu', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const onRemove = vi.fn()
    const entry = makeEntry({ query: 'one', id: 'o' })
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries: [entry],
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick: () => {},
        onRemove,
      },
    })
    await tick()
    const row = document.querySelector('.result-row') as HTMLElement
    const evt = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    row.dispatchEvent(evt)
    expect(onRemove).toHaveBeenCalledWith(entry)
    expect(evt.defaultPrevented).toBe(true)
    target.remove()
    anchor.remove()
  })

  it('resets the filter every time the popover reopens', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    // Start closed, then open, type, close, re-open. Re-opens via remount because Svelte 5's
    // top-level `open` prop is read once per mount in this test harness.
    const props = {
      anchor,
      open: true,
      entries: [makeEntry({ query: 'one' })],
      adapter: searchAdapter,
      keyFn: searchKey,
      onClose: () => {},
      onPick: () => {},
      onRemove: () => {},
    }
    const component = mount(RecentSearchesPopover, { target, props })
    await tick()
    const input = document.querySelector('.search-field') as HTMLInputElement
    input.value = 'zzz'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    void unmount(component)

    // Fresh mount = closed → opened → clean filter.
    mount(RecentSearchesPopover, { target, props: { ...props, open: false } })
    await tick()
    const remount = mount(RecentSearchesPopover, { target, props: { ...props, open: true } })
    await tick()
    const freshInput = document.querySelector<HTMLInputElement>('.search-field')
    expect(freshInput?.value).toBe('')
    void unmount(remount)
    target.remove()
    anchor.remove()
  })
})
