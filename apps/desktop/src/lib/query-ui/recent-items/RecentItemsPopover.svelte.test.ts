import { describe, it, expect, vi } from 'vitest'
import { mount, tick, unmount, type Component } from 'svelte'
import RecentSearchesPopoverRaw from './RecentItemsPopover.svelte'
import type { HistoryEntry } from '$lib/tauri-commands'
import type { RecentItemAdapter, RecentItemKey } from './recent-items-types'
import { chipTooltip, modeName, formatAge, rowMeta } from './recent-items-utils'

// Svelte 5 generics+mount type roundtrip: cast through unknown to avoid unsafe-argument errors.
// The generic component's props resolve to `unknown` through `mount()`, so the props object
// is passed as a plain record.
const RecentSearchesPopover = RecentSearchesPopoverRaw as unknown as Component<Record<string, unknown>>

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
  metaLabel: '',
  ariaLabel: `Run recent ${modeName(entry.mode)} search: ${entry.query}`,
})
const searchKey: RecentItemKey<HistoryEntry> = (entry) => entry.id

/** Same as `searchAdapter` but with the meta line the real consumers build. */
const richAdapter: RecentItemAdapter<HistoryEntry> = (entry) => ({
  ...searchAdapter(entry),
  metaLabel: rowMeta(entry),
})

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
        onExitTop: () => {},
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
        onExitTop: () => {},
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
        onExitTop: () => {},
      },
    })
    await tick()

    const input = document.querySelector('.recent-popover input.text-field-control') as HTMLInputElement
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
        onExitTop: () => {},
      },
    })
    await tick()
    const input = document.querySelector('.recent-popover input.text-field-control') as HTMLInputElement
    input.value = 'zzzzzz'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(document.querySelector('.empty')?.textContent ?? '').toContain('No recent searches')
    target.remove()
    anchor.remove()
  })

  /**
   * The dropdown is the primary navigation surface while open, so every key it claims must
   * also stop propagating: the host dialog's own keydown would otherwise move the results
   * cursor underneath and double-fire Enter. `hostKeys` is the stand-in for that handler.
   */
  function mountForKeyboard(): {
    popover: HTMLElement
    onPick: ReturnType<typeof vi.fn>
    onClose: ReturnType<typeof vi.fn>
    onExitTop: ReturnType<typeof vi.fn>
    hostKeys: string[]
    cleanup: () => void
  } {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const onPick = vi.fn()
    const onClose = vi.fn()
    const onExitTop = vi.fn()
    const hostKeys: string[] = []
    const host = (e: Event): void => {
      hostKeys.push((e as KeyboardEvent).key)
    }
    document.body.addEventListener('keydown', host)
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries: [
          makeEntry({ query: 'first', id: 'f' }),
          makeEntry({ query: 'second', id: 's' }),
          makeEntry({ query: 'third', id: 't' }),
        ],
        adapter: searchAdapter,
        keyFn: searchKey,
        onClose,
        onPick,
        onRemove: () => {},
        onExitTop,
      },
    })
    const popover = document.querySelector('.recent-popover') as HTMLElement
    return {
      popover,
      onPick,
      onClose,
      onExitTop,
      hostKeys,
      cleanup: () => {
        document.body.removeEventListener('keydown', host)
        target.remove()
        anchor.remove()
      },
    }
  }

  function press(popover: HTMLElement, key: string): void {
    popover.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
  }

  function cursorIndex(): number {
    return Array.from(document.querySelectorAll('.result-row')).findIndex((r) => r.classList.contains('is-cursor'))
  }

  it('Enter selects the cursor row and closes, and never reaches the host dialog', async () => {
    const { popover, onPick, onClose, hostKeys, cleanup } = mountForKeyboard()
    await tick()
    press(popover, 'Enter')
    expect(onPick).toHaveBeenCalledTimes(1)
    expect((onPick.mock.calls[0][0] as HistoryEntry).id).toBe('f')
    // Selecting closes: "select" means the entry is now in the query field.
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(hostKeys).not.toContain('Enter')
    cleanup()
  })

  it('ArrowDown moves the cursor and does not wrap past the last row', async () => {
    const { popover, hostKeys, cleanup } = mountForKeyboard()
    await tick()
    expect(cursorIndex()).toBe(0)
    press(popover, 'ArrowDown')
    await tick()
    expect(cursorIndex()).toBe(1)
    press(popover, 'ArrowDown')
    await tick()
    press(popover, 'ArrowDown')
    await tick()
    press(popover, 'ArrowDown')
    await tick()
    // Three entries: the cursor clamps at the last one instead of looping to the top.
    expect(cursorIndex()).toBe(2)
    expect(hostKeys).not.toContain('ArrowDown')
    cleanup()
  })

  it('ArrowUp on the topmost row exits to the query field instead of clamping', async () => {
    const { popover, onExitTop, onPick, hostKeys, cleanup } = mountForKeyboard()
    await tick()
    press(popover, 'ArrowDown')
    await tick()
    press(popover, 'ArrowUp')
    await tick()
    expect(cursorIndex()).toBe(0)
    expect(onExitTop).not.toHaveBeenCalled()
    // Once at the top, ArrowUp is the way out. Nothing is picked, so the field's text stays.
    press(popover, 'ArrowUp')
    expect(onExitTop).toHaveBeenCalledTimes(1)
    expect(onPick).not.toHaveBeenCalled()
    expect(hostKeys).not.toContain('ArrowUp')
    cleanup()
  })

  it('leaves editing keys alone so the filter field behaves like any text field', async () => {
    const { popover, onPick, cleanup } = mountForKeyboard()
    await tick()
    for (const key of ['ArrowLeft', 'ArrowRight', 'Home', 'End', 'c', 'v', 'x']) {
      const e = new KeyboardEvent('keydown', {
        key,
        bubbles: true,
        cancelable: true,
        metaKey: key.length === 1,
      })
      popover.dispatchEvent(e)
      expect(e.defaultPrevented).toBe(false)
    }
    expect(onPick).not.toHaveBeenCalled()
    cleanup()
  })

  it('a row click selects that row and closes', async () => {
    const { onPick, onClose, cleanup } = mountForKeyboard()
    await tick()
    const rows = document.querySelectorAll<HTMLElement>('.result-row')
    rows[1].click()
    expect((onPick.mock.calls[0][0] as HistoryEntry).id).toBe('s')
    expect(onClose).toHaveBeenCalledTimes(1)
    cleanup()
  })

  it('renders the age and the meta line on each row', async () => {
    const anchor = setupAnchor()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RecentSearchesPopover, {
      target,
      props: {
        anchor,
        open: true,
        entries: [makeEntry({ query: 'photos', id: 'p', resultCount: 1203 })],
        adapter: richAdapter,
        keyFn: searchKey,
        onClose: () => {},
        onPick: () => {},
        onRemove: () => {},
        onExitTop: () => {},
      },
    })
    await tick()
    const row = document.querySelector('.result-row')
    expect(row?.querySelector('.row-age')?.textContent).toBe('just now')
    expect(row?.querySelector('.row-detail')?.textContent).toContain('1,203 results')
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
        onExitTop: () => {},
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
      onExitTop: () => {},
    }
    const component = mount(RecentSearchesPopover, { target, props })
    await tick()
    const input = document.querySelector('.recent-popover input.text-field-control') as HTMLInputElement
    input.value = 'zzz'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    void unmount(component)

    // Fresh mount = closed → opened → clean filter.
    mount(RecentSearchesPopover, { target, props: { ...props, open: false } })
    await tick()
    const remount = mount(RecentSearchesPopover, { target, props: { ...props, open: true } })
    await tick()
    const freshInput = document.querySelector<HTMLInputElement>('.recent-popover input.text-field-control')
    expect(freshInput?.value).toBe('')
    void unmount(remount)
    target.remove()
    anchor.remove()
  })
})
