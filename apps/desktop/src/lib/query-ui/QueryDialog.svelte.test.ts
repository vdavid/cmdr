/**
 * Behavior tests for `QueryDialog.svelte`, the shared orchestrator.
 *
 * Pins the M4 ownership contracts and the keyboard / IME / action wiring against a
 * minimal Search-shaped config. Search's full integration is covered by
 * `lib/search/SearchDialog.svelte.test.ts` (which mounts QueryDialog through the thin
 * Search wrapper); these tests target the orchestrator's contract directly so
 * regressions there don't cascade through every consumer.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import { writable } from 'svelte/store'
import QueryDialog from './QueryDialog.svelte'
import { createQueryFilterState, type QueryFilterState } from './query-filter-state.svelte'
import { createRecentItemsState } from './recent-items/recent-items-state.svelte'
import type { QueryDialogConfig, AiTranslateResult } from './query-dialog-config'
import type { SearchResultEntry } from '$lib/tauri-commands'
import type { HistoryEntry } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => {
    if (key === 'search.autoApply') return true
    return undefined
  }),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/icon-cache', () => ({
  iconCacheVersion: writable(0),
  getCachedIcon: vi.fn(() => undefined),
}))

interface MountedDialog {
  overlay: Element
  state: QueryFilterState
  cleanup: () => void
  config: QueryDialogConfig<HistoryEntry>
  calls: {
    primary: SearchResultEntry[][]
    secondary: SearchResultEntry[]
    runQuery: number
    translateAi: string[]
    clearState: number
    close: number
  }
}

interface MountOptions {
  runQueryResult?: { entries: SearchResultEntry[]; totalCount: number }
  translateAi?: (prompt: string) => Promise<AiTranslateResult | null>
  initialQuery?: string
  initialMode?: 'ai' | 'filename' | 'regex'
  recentEntries?: HistoryEntry[]
}

function mountQueryDialog(opts: MountOptions = {}): MountedDialog {
  const state = createQueryFilterState({ defaultMode: 'filename' })
  if (opts.initialQuery !== undefined) state.setQuery(opts.initialQuery)
  if (opts.initialMode !== undefined) state.setMode(opts.initialMode)

  const calls = {
    primary: [] as SearchResultEntry[][],
    secondary: [] as SearchResultEntry[],
    runQuery: 0,
    translateAi: [] as string[],
    clearState: 0,
    close: 0,
  }

  const historyStore = createRecentItemsState<HistoryEntry>({
    getRecent: () => Promise.resolve(opts.recentEntries ?? []),
  })
  if (opts.recentEntries) historyStore.setList(opts.recentEntries)

  const config: QueryDialogConfig<HistoryEntry> = {
    title: 'Test dialog',
    dialogType: 'test',
    maxWidth: 'min(800px, 80vw)',
    state,
    aiEnabled: true,
    inputsDisabled: false,
    visibleChips: { size: true, date: true, scope: true, pattern: true },
    showPathColumn: true,
    runHintCopy: 'Press Enter to search',
    historyStore,
    recentItems: {
      adapter: (e) => ({
        label: e.query,
        tooltip: e.query,
        mode: e.mode,
        ageLabel: 'now',
        ariaLabel: e.query,
      }),
      keyFn: (e) => e.id,
    },
    emptyState: { examples: [], indexEntryCount: 1000 },
    filterChipsExtras: {
      caseSensitive: false,
      scope: '',
      excludeSystemDirs: true,
      searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
      systemDirExcludeTooltip: '',
      aiPattern: null,
      onToggleCaseSensitive: () => {},
      onToggleExcludeSystemDirs: () => {},
      onSetScope: () => {},
      onClearAiPattern: () => {},
    },
    scanning: false,
    entriesScanned: 0,
    indexEntryCount: 1000,
    isIndexAvailable: true,
    isIndexReady: true,
    runQuery: () => {
      calls.runQuery += 1
      return Promise.resolve(opts.runQueryResult ?? { entries: [], totalCount: 0 })
    },
    translateAi: opts.translateAi
      ? (() => {
          const fn = opts.translateAi
          return async (prompt: string) => {
            calls.translateAi.push(prompt)
            return fn(prompt)
          }
        })()
      : undefined,
    primaryAction: {
      label: 'Primary',
      shortcutHint: '⌥⏎',
      handler: (entries) => {
        calls.primary.push(entries)
      },
    },
    secondaryAction: {
      label: 'Secondary',
      shortcutHint: '⏎',
      handler: (entry) => {
        calls.secondary.push(entry)
      },
    },
    onPickPath: () => {},
    onPickExample: () => {},
    onRowMenu: () => {},
    onActivateRecent: () => {},
    onRemoveRecent: () => {},
    onClose: () => {
      calls.close += 1
    },
    onClearState: () => {
      calls.clearState += 1
    },
  }

  const target = document.createElement('div')
  document.body.appendChild(target)
  // Svelte's `mount()` typing of a generic component pins the type parameter at the
  // call site; we widen via `unknown` so the test's `HistoryEntry`-typed config still
  // passes the type check without losing inference on the rest of the file.
  const component = mount(QueryDialog, { target, props: { config: config } })

  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('overlay not found')

  return {
    overlay,
    state,
    config,
    calls,
    cleanup: () => {
      void unmount(component)
      target.remove()
    },
  }
}

function dispatchKey(
  target: Element,
  key: string,
  mods: { meta?: boolean; alt?: boolean; shift?: boolean } = {},
): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    metaKey: mods.meta ?? false,
    altKey: mods.alt ?? false,
    shiftKey: mods.shift ?? false,
    bubbles: true,
    cancelable: true,
  })
  target.dispatchEvent(event)
  return event
}

const SAMPLE_RESULT: SearchResultEntry = {
  name: 'photo.jpg',
  path: '/Users/test/photo.jpg',
  parentPath: '/Users/test',
  isDirectory: false,
  size: 1000,
  modifiedAt: 1_700_000_000,
  iconId: 'ext:jpg',
}

describe('QueryDialog title bar', () => {
  it('renders the configured title in the header', async () => {
    const { overlay, cleanup } = mountQueryDialog()
    await tick()
    const title = overlay.querySelector('#query-dialog-title')
    expect(title?.textContent).toContain('Test dialog')
    cleanup()
  })
})

describe('QueryDialog primary / secondary actions', () => {
  it('⌥⏎ fires primaryAction.handler with the current results', async () => {
    const { overlay, state, calls, cleanup } = mountQueryDialog({
      runQueryResult: { entries: [SAMPLE_RESULT], totalCount: 1 },
    })
    await tick()
    await Promise.resolve()
    await tick()
    state.setResults([SAMPLE_RESULT])
    state.setTotalCount(1)
    state.setCursorIndex(0)
    await tick()

    dispatchKey(overlay, 'Enter', { alt: true })
    await tick()

    expect(calls.primary.length).toBe(1)
    expect(calls.primary[0]).toEqual([SAMPLE_RESULT])
    cleanup()
  })

  it('⏎ fires secondaryAction.handler with the cursor entry when enterAction is go-to-file', async () => {
    const { overlay, state, calls, cleanup } = mountQueryDialog()
    await tick()
    await Promise.resolve()
    await tick()
    // Seed results and mark as "just arrived" so deriveEnterAction returns 'go-to-file'.
    state.setResults([SAMPLE_RESULT])
    state.setTotalCount(1)
    state.setCursorIndex(0)
    state.setLastDialogEvent('results-arrived')
    await tick()

    dispatchKey(overlay, 'Enter')
    await tick()

    expect(calls.secondary.length).toBe(1)
    expect(calls.secondary[0]).toEqual(SAMPLE_RESULT)
    cleanup()
  })
})

describe('QueryDialog ⌘N and ⌘H', () => {
  it('⌘N invokes the consumer onClearState hook', async () => {
    const { overlay, calls, cleanup } = mountQueryDialog()
    await tick()
    await Promise.resolve()
    await tick()

    dispatchKey(overlay, 'n', { meta: true })
    await tick()

    expect(calls.clearState).toBe(1)
    cleanup()
  })

  it('⌘H toggles the recent-items popover open', async () => {
    const entry: HistoryEntry = {
      id: 'h1',
      timestamp: Date.now(),
      mode: 'filename',
      query: '*.pdf',
      filters: { sizeMin: null, sizeMax: null, modifiedAfter: null, modifiedBefore: null },
      scope: '',
      caseSensitive: false,
      excludeSystemDirs: true,
      resultCount: 0,
    }
    const { overlay, cleanup } = mountQueryDialog({ recentEntries: [entry] })
    await tick()
    await Promise.resolve()
    await tick()

    expect(document.body.querySelector('.recent-items-popover, .recent-searches-popover')).toBeNull()

    dispatchKey(overlay, 'h', { meta: true })
    await tick()

    // The popover mounts via FilterChipPopover; either marker class would work,
    // but the wrapper exposes the `[data-recent-items-popover]` hook below.
    const popoverAfterOpen = document.body.querySelector(
      '[data-recent-items-popover], .recent-searches-popover, .filter-chip-popover',
    )
    expect(popoverAfterOpen).not.toBeNull()

    cleanup()
  })
})

describe('QueryDialog IME composition guard', () => {
  it('compositionstart suppresses auto-apply; compositionend triggers exactly one fire', async () => {
    const { state, calls, cleanup } = mountQueryDialog()
    await tick()
    await Promise.resolve()
    await tick()

    vi.useFakeTimers()
    try {
      const input = document.body.querySelector('input.query-input') as HTMLInputElement
      expect(input).not.toBeNull()

      input.dispatchEvent(new CompositionEvent('compositionstart'))
      // Simulate composing keystrokes via the bar's input handler.
      input.value = 'ｐ'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.value = 'ｐｈ'
      input.dispatchEvent(new Event('input', { bubbles: true }))

      vi.advanceTimersByTime(2_000)
      await Promise.resolve()
      expect(calls.runQuery).toBe(0)

      input.dispatchEvent(new CompositionEvent('compositionend'))
      vi.advanceTimersByTime(999)
      expect(calls.runQuery).toBe(0)
      vi.advanceTimersByTime(1)
      await Promise.resolve()
      await Promise.resolve()
      expect(calls.runQuery).toBe(1)
    } finally {
      vi.useRealTimers()
    }
    // State got the typed value.
    expect(state.getQuery()).toBe('ｐｈ')
    cleanup()
  })
})

describe('QueryDialog lastDialogEvent ownership', () => {
  it("writes 'opened' on mount", async () => {
    const { state, cleanup } = mountQueryDialog()
    await tick()
    await Promise.resolve()
    await tick()
    expect(state.getLastDialogEvent()).toBe('opened')
    cleanup()
  })

  it("writes 'results-arrived' after a runQuery completes even when the consumer never touches it", async () => {
    const { overlay, state, calls, cleanup } = mountQueryDialog({
      runQueryResult: { entries: [SAMPLE_RESULT], totalCount: 1 },
    })
    await tick()
    await Promise.resolve()
    await tick()
    // Take the dialog out of the 'opened' state by editing the query, then drive a run.
    state.setQuery('*.pdf')
    state.setLastDialogEvent('query-edited')
    await tick()

    dispatchKey(overlay, 'Enter')
    // Let runQuery's promise settle.
    await tick()
    await Promise.resolve()
    await tick()

    expect(calls.runQuery).toBe(1)
    expect(state.getLastDialogEvent()).toBe('results-arrived')
    expect(state.getResults()).toEqual([SAMPLE_RESULT])
    cleanup()
  })

  it("writes 'query-edited' on bar input", async () => {
    const { state, cleanup } = mountQueryDialog()
    await tick()
    await Promise.resolve()
    await tick()
    const input = document.body.querySelector('input.query-input') as HTMLInputElement
    expect(input).not.toBeNull()
    input.value = 'p'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(state.getLastDialogEvent()).toBe('query-edited')
    cleanup()
  })
})
