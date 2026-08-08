/**
 * Behavior tests for `QueryDialog.svelte`, the shared orchestrator.
 *
 * Pins the ownership contracts and the keyboard / IME / action wiring against a
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
import { getToasts, clearAllToasts } from '$lib/ui/toast/toast-store.svelte'
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
    activateRecent: HistoryEntry[]
  }
}

interface MountOptions {
  badge?: 'alpha' | 'beta'
  runQueryResult?: { entries: SearchResultEntry[]; totalCount: number }
  runQueryError?: Error
  translateAi?: (prompt: string) => Promise<AiTranslateResult | null>
  initialQuery?: string
  initialMode?: 'ai' | 'filename' | 'regex'
  recentEntries?: HistoryEntry[]
  /** Wires the Search-only count-only mode (Selection leaves both undefined). */
  countOnly?: boolean
  onToggleCountOnly?: () => void
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
    activateRecent: [] as HistoryEntry[],
  }

  const historyStore = createRecentItemsState<HistoryEntry>({
    getRecent: () => Promise.resolve(opts.recentEntries ?? []),
  })
  if (opts.recentEntries) historyStore.setList(opts.recentEntries)

  const config: QueryDialogConfig<HistoryEntry> = {
    title: 'Test dialog',
    badge: opts.badge,
    // A real registered id: `dialogType` is a `SoftDialogId`, so no placeholder here.
    dialogType: 'search',
    width: 'min(800px, 80vw)',
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
        metaLabel: '12 results',
        ariaLabel: e.query,
      }),
      keyFn: (e) => e.id,
    },
    emptyState: { examples: [], indexEntryCount: 1000 },
    filterChipsExtras: {
      caseSensitive: false,
      scope: '',
      excludeSystemDirs: true,
      scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      defaultScope: { path: '/Users/test', label: 'Current folder' },
      systemDirExcludeTooltip: '',
      aiPattern: null,
      aiPatternKind: null,
      onToggleCaseSensitive: () => {},
      onToggleExcludeSystemDirs: () => {},
      onSetScope: () => {},
      onClearAiPattern: () => {},
      countOnly: opts.countOnly,
      onToggleCountOnly: opts.onToggleCountOnly,
    },
    scanning: false,
    entriesScanned: 0,
    indexEntryCount: 1000,
    isIndexAvailable: true,
    isIndexReady: true,
    runQuery: () => {
      calls.runQuery += 1
      if (opts.runQueryError !== undefined) return Promise.reject(opts.runQueryError)
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
    onActivateRecent: (entry) => {
      calls.activateRecent.push(entry)
    },
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
  const component = mount(QueryDialog, {
    target,
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion -- load-bearing for svelte-check; `mount()` erases the generic to its `unknown` default and `historyStore.setList`'s contravariant `next` arg can't be re-narrowed. ESLint's `--fix` keeps stripping the cast (see commit 9962b00c).
    props: { config: config as unknown as QueryDialogConfig },
  })

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

const RECENT_ENTRY: HistoryEntry = {
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

/** Mount does async work (history load, consumer onMount, focus); let it drain. */
async function settle(): Promise<void> {
  await tick()
  await Promise.resolve()
  await tick()
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

  it('renders a status badge next to the title when config.badge is set', async () => {
    const { overlay, cleanup } = mountQueryDialog({ badge: 'alpha' })
    await tick()
    const badge = overlay.querySelector('#query-dialog-title .feature-status-badge')
    expect(badge?.textContent).toBe('alpha')
    cleanup()
  })

  it('renders no status badge when config.badge is unset', async () => {
    const { overlay, cleanup } = mountQueryDialog()
    await tick()
    expect(overlay.querySelector('#query-dialog-title .feature-status-badge')).toBeNull()
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

  it('⌘H opens the recent-items dropdown, and toggles it shut again', async () => {
    const { overlay, cleanup } = mountQueryDialog({ recentEntries: [RECENT_ENTRY] })
    await settle()

    expect(document.body.querySelector('.recent-popover')).toBeNull()

    dispatchKey(overlay, 'h', { meta: true })
    await tick()
    expect(document.body.querySelector('.recent-popover')).not.toBeNull()

    dispatchKey(overlay, 'h', { meta: true })
    await tick()
    expect(document.body.querySelector('.recent-popover')).toBeNull()

    cleanup()
  })
})

/**
 * The query field is a combobox over the recent-items history: the chevron, `⌘H`, and
 * `ArrowDown` (when there's no result list to walk) all open the same dropdown, and picking
 * a row LOADS the entry without running it.
 */
describe('QueryDialog recent-items dropdown', () => {
  it('ArrowDown opens the dropdown when there are no results to walk', async () => {
    const { overlay, cleanup } = mountQueryDialog({ recentEntries: [RECENT_ENTRY] })
    await settle()

    dispatchKey(overlay, 'ArrowDown')
    await tick()
    expect(document.body.querySelector('.recent-popover')).not.toBeNull()
    cleanup()
  })

  it('ArrowDown keeps walking the results when there are some', async () => {
    const { overlay, state, cleanup } = mountQueryDialog({
      recentEntries: [RECENT_ENTRY],
      // A runnable query: an empty bar with no filter is refused before it reaches `runQuery`.
      initialQuery: '*.jpg',
      runQueryResult: { entries: [SAMPLE_RESULT, { ...SAMPLE_RESULT, path: '/b', name: 'b.jpg' }], totalCount: 2 },
    })
    await settle()
    dispatchKey(overlay, 'Enter')
    await settle()
    expect(state.getResults().length).toBe(2)

    dispatchKey(overlay, 'ArrowDown')
    await tick()
    expect(document.body.querySelector('.recent-popover')).toBeNull()
    expect(state.getCursorIndex()).toBe(1)
    cleanup()
  })

  it('the field chevron opens the dropdown', async () => {
    const { overlay, cleanup } = mountQueryDialog({ recentEntries: [RECENT_ENTRY] })
    await settle()

    const trigger = overlay.querySelector('.query-bar .recent-trigger') as HTMLButtonElement
    expect(trigger).not.toBeNull()
    trigger.click()
    await tick()
    expect(document.body.querySelector('.recent-popover')).not.toBeNull()
    cleanup()
  })

  it('ArrowUp on the top row closes the dropdown and hands focus back to the field, text intact', async () => {
    const { overlay, calls, state, cleanup } = mountQueryDialog({
      recentEntries: [RECENT_ENTRY],
      initialQuery: 'half-typed',
    })
    await settle()
    const runsBefore = calls.runQuery

    dispatchKey(overlay, 'h', { meta: true })
    await tick()
    const popover = document.body.querySelector('.recent-popover')
    expect(popover).not.toBeNull()

    // The cursor opens on the top row, so the first ArrowUp is the exit.
    dispatchKey(popover as Element, 'ArrowUp')
    await settle()

    expect(document.body.querySelector('.recent-popover')).toBeNull()
    // Exiting is navigation, not selection: nothing loaded, nothing run, text untouched.
    expect(calls.activateRecent).toEqual([])
    expect(calls.runQuery).toBe(runsBefore)
    expect(state.getQuery()).toBe('half-typed')
    expect(document.activeElement).toBe(overlay.querySelector('.query-bar input.text-field-control'))
    cleanup()
  })

  it('picking a row loads the entry, closes the dropdown, and does NOT run the query', async () => {
    const { overlay, calls, state, cleanup } = mountQueryDialog({ recentEntries: [RECENT_ENTRY] })
    await settle()
    const runsBefore = calls.runQuery

    dispatchKey(overlay, 'h', { meta: true })
    await tick()
    const row = document.body.querySelector('.recent-popover .result-row') as HTMLButtonElement
    expect(row).not.toBeNull()
    row.click()
    await tick()

    expect(calls.activateRecent).toEqual([RECENT_ENTRY])
    // Selecting never runs: a recent search is a starting point, and an AI entry that
    // re-translated itself would bill the user for a keystroke.
    expect(calls.runQuery).toBe(runsBefore)
    expect(state.getRunOnMount()).toBe(false)
    expect(document.body.querySelector('.recent-popover')).toBeNull()
    // ⏎ goes back to owning "run-search" so the very next Enter runs what was picked.
    expect(state.getLastDialogEvent()).toBe('query-edited')
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
      const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement
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
    const input = document.body.querySelector('.query-bar input.text-field-control') as HTMLInputElement
    expect(input).not.toBeNull()
    input.value = 'p'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(state.getLastDialogEvent()).toBe('query-edited')
    cleanup()
  })
})

describe('QueryDialog count-only switch', () => {
  it('renders no switch when the consumer omits onToggleCountOnly (Selection)', async () => {
    const { overlay, cleanup } = mountQueryDialog()
    await tick()
    expect(overlay.querySelector('.query-grid__count-only [role="switch"]')).toBeNull()
    cleanup()
  })

  it('renders it beside the mode chips, reflecting the state (Search)', async () => {
    const { overlay, cleanup } = mountQueryDialog({ countOnly: true, onToggleCountOnly: () => {} })
    await tick()
    const sw = overlay.querySelector('.query-grid__count-only [role="switch"]')
    expect(sw).not.toBeNull()
    expect((sw as HTMLInputElement).checked).toBe(true)
    cleanup()
  })

  it('flipping it toggles count-only and re-runs, without an AI call', async () => {
    const onToggleCountOnly = vi.fn()
    // A runnable query: flipping the switch on an empty bar with no filter has nothing to re-run.
    const { overlay, calls, cleanup } = mountQueryDialog({
      countOnly: false,
      onToggleCountOnly,
      initialQuery: '*.jpg',
    })
    await tick()
    await Promise.resolve()
    await tick()

    vi.useFakeTimers()
    try {
      overlay.querySelector<HTMLInputElement>('.query-grid__count-only [role="switch"]')?.click()
      expect(onToggleCountOnly).toHaveBeenCalledOnce()
      // Debounced (`scheduleSearch`), which is what keeps AI mode's explicit-trigger contract.
      vi.advanceTimersByTime(1_000)
      await Promise.resolve()
      expect(calls.runQuery).toBe(1)
    } finally {
      vi.useRealTimers()
    }
    cleanup()
  })
})

describe('QueryDialog "Show results" under a count-only total', () => {
  // The count-only run returned no rows, so flipping the flag alone leaves a stale number on
  // screen. The re-run must also NOT go through the debounce, which no-ops when auto-apply is off.
  it('turns count-only off and re-runs immediately', async () => {
    let countOnly = true
    const { overlay, state, calls, cleanup } = mountQueryDialog({
      countOnly,
      onToggleCountOnly: () => {
        countOnly = !countOnly
      },
    })
    await tick()
    await Promise.resolve()
    await tick()
    // Land in the count-only content state: a run has happened and there's a query.
    state.setQuery('*.jpg')
    dispatchKey(overlay, 'Enter')
    await tick()
    await Promise.resolve()
    await tick()
    const runsBefore = calls.runQuery

    const button = overlay.querySelector<HTMLButtonElement>('.count-only-summary button')
    expect(button?.textContent.trim()).toBe('Show results')
    button?.click()
    await tick()
    await Promise.resolve()

    expect(countOnly).toBe(false)
    // No timers advanced: the re-run fired straight away.
    expect(calls.runQuery).toBe(runsBefore + 1)
    cleanup()
  })
})

/**
 * An empty bar AND every filter at its default is not a query — the backend refuses it
 * ("Query too broad"), and before the guard that refusal surfaced as a warning toast the
 * moment the user cleared the field. An empty pattern WITH an active filter stays a
 * legitimate query (`≥ 1 MB` selects every file ≥ 1 MB); only "nothing at all" is guarded.
 */
describe('QueryDialog nothing-to-run guard', () => {
  /** Scoped to this dialog's own overlay: a failed sibling test can leave a stale one behind. */
  function typeQuery(overlay: Element, value: string): void {
    const input = overlay.querySelector<HTMLInputElement>('.query-bar input.text-field-control')
    if (!input) throw new Error('query input not found')
    input.value = value
    input.dispatchEvent(new Event('input', { bubbles: true }))
  }

  it('clearing the query with no active filter never runs, and drops the stale rows', async () => {
    clearAllToasts()
    const { overlay, state, calls, cleanup } = mountQueryDialog({
      initialQuery: '*.jpg',
      runQueryResult: { entries: [SAMPLE_RESULT], totalCount: 1 },
    })
    await settle()
    dispatchKey(overlay, 'Enter')
    await settle()
    expect(calls.runQuery).toBe(1)
    expect(state.getResults().length).toBe(1)

    vi.useFakeTimers()
    try {
      typeQuery(overlay, '')
      vi.advanceTimersByTime(2_000)
      await Promise.resolve()
      await Promise.resolve()
    } finally {
      vi.useRealTimers()
    }
    await settle()

    // No doomed second run, so no "Query too broad" toast.
    expect(calls.runQuery).toBe(1)
    expect(getToasts()).toEqual([])
    // And the previous run's rows don't sit there implying they still match.
    expect(state.getResults()).toEqual([])
    expect(state.getTotalCount()).toBe(0)
    cleanup()
  })

  it('an empty pattern WITH an active size filter still runs (filter-only query)', async () => {
    const { overlay, state, calls, cleanup } = mountQueryDialog({
      initialQuery: '*.jpg',
      runQueryResult: { entries: [SAMPLE_RESULT], totalCount: 1 },
    })
    await settle()
    state.setSizeFilter('gte')
    await tick()

    vi.useFakeTimers()
    try {
      typeQuery(overlay, '')
      vi.advanceTimersByTime(2_000)
      await Promise.resolve()
      await Promise.resolve()
    } finally {
      vi.useRealTimers()
    }
    await settle()

    expect(calls.runQuery).toBe(1)
    expect(state.getResults()).toEqual([SAMPLE_RESULT])
    cleanup()
  })

  it('a non-default type filter alone counts as runnable', async () => {
    const { overlay, state, calls, cleanup } = mountQueryDialog({
      initialQuery: '*.jpg',
      runQueryResult: { entries: [SAMPLE_RESULT], totalCount: 1 },
    })
    await settle()
    state.setTypeFilter('folder')
    await tick()

    vi.useFakeTimers()
    try {
      typeQuery(overlay, '')
      vi.advanceTimersByTime(2_000)
      await Promise.resolve()
      await Promise.resolve()
    } finally {
      vi.useRealTimers()
    }
    await settle()

    expect(calls.runQuery).toBe(1)
    cleanup()
  })
})

describe('QueryDialog run failures', () => {
  it('surfaces the reason a run was refused instead of showing an empty list', async () => {
    clearAllToasts()
    const { overlay, state, cleanup } = mountQueryDialog({
      runQueryError: new Error('Query too broad. Add a filename pattern, size, date, or type filter'),
    })
    await tick()
    await Promise.resolve()
    await tick()
    state.setQuery('*')
    dispatchKey(overlay, 'Enter')
    await tick()
    await Promise.resolve()
    await tick()

    const messages = getToasts().map((t) => String(t.content))
    // Pre-fix the catch was bare, so a refused run read as "nothing matched".
    expect(messages.some((m) => m.includes('Query too broad'))).toBe(true)
    clearAllToasts()
    cleanup()
  })
})

describe('QueryDialog chrome', () => {
  it('is a ModalDialog: standard panel, close button, and the .search-overlay hook', async () => {
    const { overlay, calls, cleanup } = mountQueryDialog()
    await tick()
    // The E2E suite and the overlay-dismissal safety net key on `.search-overlay`.
    expect(overlay.classList.contains('modal-overlay')).toBe(true)
    expect(overlay.querySelector('.modal-dialog')).not.toBeNull()
    const close = overlay.querySelector<HTMLButtonElement>('.modal-close-button')
    expect(close).not.toBeNull()
    close?.click()
    expect(calls.close).toBe(1)
    cleanup()
  })

  it('keeps Escape and Enter for itself (ownsKeyboard)', async () => {
    const { overlay, state, calls, cleanup } = mountQueryDialog()
    await tick()
    await Promise.resolve()
    await tick()
    // Enter runs the query rather than being swallowed by ModalDialog's button short-circuit.
    state.setQuery('*.pdf')
    dispatchKey(overlay, 'Enter')
    await tick()
    await Promise.resolve()
    expect(calls.runQuery).toBe(1)

    dispatchKey(overlay, 'Escape')
    await tick()
    // Exactly once: the dialog's capture-phase handler closes, ModalDialog's doesn't double-fire.
    expect(calls.close).toBe(1)
    cleanup()
  })
})

describe('QueryDialog focus restore', () => {
  it('returns focus to the previously focused element on unmount', async () => {
    const outside = document.createElement('button')
    document.body.appendChild(outside)
    outside.focus()
    const { cleanup } = mountQueryDialog()
    // Let the async onMount settle and move focus into the dialog's input.
    await tick()
    await Promise.resolve()
    await tick()
    expect(document.activeElement).not.toBe(outside)
    cleanup()
    await tick()
    // Pre-fix this landed on <body>, leaving pane keyboard nav dead after Escape.
    expect(document.activeElement).toBe(outside)
    outside.remove()
  })
})
