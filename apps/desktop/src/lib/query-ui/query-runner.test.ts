/**
 * The dialog's run controller: the nothing-to-run guard, auto-apply gating, the AI
 * round-trip, and the two silent-failure traps (a swallowed `runQuery` rejection reads as
 * "nothing matched"; a spinner that never clears reads as a hung dialog).
 *
 * These are contract tests against a fake `QueryDialogConfig`, so they pin the ordering
 * (`lastAiPrompt` captured BEFORE the IPC, `isSearching` cleared on every early return)
 * without mounting the dialog.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import type { SearchResultEntry } from '$lib/tauri-commands'
import { clearAllToasts, getToasts } from '$lib/ui/toast/toast-store.svelte'
import type { AiTranslateResult, QueryDialogConfig } from './query-dialog-config'
import { SEARCH_AUTO_APPLY_DEBOUNCE_MS } from './query-filter-state.svelte'
import {
  AI_HIGHLIGHT_FLASH_MS,
  createQueryRunner,
  hasRunnableQuery,
  shouldShowRunHint,
  type QueryRunner,
} from './query-runner.svelte'
import { makeQueryDialogConfig, sampleEntries } from './test-helpers'

interface Harness {
  runner: QueryRunner
  config: QueryDialogConfig
  calls: { runQuery: number; translateAi: string[]; scrollCursorIntoView: number }
  setAutoApply: (enabled: boolean) => void
}

interface HarnessOptions {
  runQueryResult?: { entries: SearchResultEntry[]; totalCount: number }
  runQueryError?: unknown
  translateAi?: (prompt: string) => Promise<AiTranslateResult | null>
  autoApply?: boolean
  /** Seeds the bar. Defaults to something runnable; pass `''` to sit on the empty state. */
  query?: string
  overrides?: Partial<QueryDialogConfig>
}

function makeRunner(opts: HarnessOptions = {}): Harness {
  const calls = { runQuery: 0, translateAi: [] as string[], scrollCursorIntoView: 0 }
  let autoApply = opts.autoApply ?? true

  const config = makeQueryDialogConfig({
    runQuery: () => {
      calls.runQuery += 1
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- one case deliberately rejects with a bare string, which is what `describeRunFailure`'s fallback is for
      if ('runQueryError' in opts) return Promise.reject(opts.runQueryError)
      return Promise.resolve(opts.runQueryResult ?? { entries: [], totalCount: 0 })
    },
    translateAi: opts.translateAi
      ? (prompt: string) => {
          calls.translateAi.push(prompt)
          // eslint-disable-next-line @typescript-eslint/no-non-null-assertion -- guarded by the ternary
          return opts.translateAi!(prompt)
        }
      : undefined,
    ...opts.overrides,
  })
  config.state.setQuery(opts.query ?? 'report')

  const runner = createQueryRunner({
    getConfig: () => config,
    isAutoApplyEnabled: () => autoApply,
    scrollCursorIntoView: () => {
      calls.scrollCursorIntoView += 1
    },
  })

  return {
    runner,
    config,
    calls,
    setAutoApply: (enabled) => {
      autoApply = enabled
    },
  }
}

beforeEach(() => {
  clearAllToasts()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('hasRunnableQuery', () => {
  it('is false on a blank session', () => {
    const { config } = makeRunner({ query: '' })
    expect(hasRunnableQuery(config.state)).toBe(false)
  })

  it('is false for a whitespace-only query', () => {
    const { config } = makeRunner({ query: '   ' })
    expect(hasRunnableQuery(config.state)).toBe(false)
  })

  it('is true for a non-empty query', () => {
    const { config } = makeRunner({ query: '  report  ' })
    expect(hasRunnableQuery(config.state)).toBe(true)
  })

  it('is true for a filter-only session (size, date, or type)', () => {
    for (const apply of [
      (c: QueryDialogConfig) => {
        c.state.setSizeFilter('gte')
      },
      (c: QueryDialogConfig) => {
        c.state.setDateFilter('after')
      },
      (c: QueryDialogConfig) => {
        c.state.setTypeFilter('folder')
      },
    ]) {
      const { config } = makeRunner({ query: '' })
      apply(config)
      expect(hasRunnableQuery(config.state)).toBe(true)
    }
  })
})

describe('shouldShowRunHint', () => {
  const base = {
    inputsDisabled: false,
    query: 'report',
    lastRunQuery: null,
    mode: 'filename' as const,
    autoApplyEnabled: false,
  }

  it('hides while the inputs are disabled', () => {
    expect(shouldShowRunHint({ ...base, inputsDisabled: true })).toBe(false)
  })

  it('hides on a blank query', () => {
    expect(shouldShowRunHint({ ...base, query: '   ' })).toBe(false)
  })

  it('hides when the query already ran (whitespace aside)', () => {
    expect(shouldShowRunHint({ ...base, query: ' report ', lastRunQuery: 'report' })).toBe(false)
  })

  it('shows in AI mode, which never auto-applies', () => {
    expect(shouldShowRunHint({ ...base, mode: 'ai', autoApplyEnabled: true })).toBe(true)
  })

  it('shows when auto-apply is off, hides when it is on', () => {
    expect(shouldShowRunHint(base)).toBe(true)
    expect(shouldShowRunHint({ ...base, autoApplyEnabled: true })).toBe(false)
  })
})

/**
 * An empty bar AND every filter at its default is not a query. The backend refuses it
 * ("Query too broad"), so before the guard the refusal surfaced as a warning toast the
 * moment the user cleared the field. An empty pattern WITH an active filter stays a
 * legitimate query.
 */
describe('the nothing-to-run guard', () => {
  it("refuses the run and drops the previous run's rows", async () => {
    const { runner, config, calls } = makeRunner({ runQueryResult: { entries: sampleEntries(2), totalCount: 2 } })
    await runner.executeQuery()
    expect(calls.runQuery).toBe(1)
    expect(config.state.getResults()).toHaveLength(2)

    config.state.setQuery('')
    await runner.executeQuery()

    expect(calls.runQuery).toBe(1)
    expect(getToasts()).toEqual([])
    expect(config.state.getResults()).toEqual([])
    expect(config.state.getTotalCount()).toBe(0)
    expect(config.state.getCursorIndex()).toBe(0)
    expect(config.state.getLastRunQuery()).toBeNull()
    expect(config.state.getIsSearching()).toBe(false)
    // Back to the empty state (examples + hints), not an empty result list.
    expect(runner.hasSearched).toBe(false)
  })

  it('drops a stale AI strip along with the rows', async () => {
    const { runner, config } = makeRunner({ query: '' })
    config.state.setLastAiPrompt('big photos')
    config.state.setLastAiCaveat('Guessed the size')
    await runner.executeQuery()
    expect(config.state.getLastAiPrompt()).toBeNull()
    expect(config.state.getLastAiCaveat()).toBeNull()
  })

  it('still runs an empty pattern that carries an active filter', async () => {
    const { runner, config, calls } = makeRunner({ query: '' })
    config.state.setSizeFilter('gte')
    await runner.executeQuery()
    expect(calls.runQuery).toBe(1)
  })

  it('heads the auto-apply chain, so clearing the bar clears the list', async () => {
    vi.useFakeTimers()
    const { runner, config, calls } = makeRunner({ runQueryResult: { entries: sampleEntries(2), totalCount: 2 } })
    await runner.executeQuery()
    expect(config.state.getResults()).toHaveLength(2)

    config.state.setQuery('')
    runner.scheduleSearch()
    // Immediate, not debounced: the rows can't sit there for a second implying they match.
    expect(config.state.getResults()).toEqual([])
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(1)
  })

  it('gates auto-apply even in AI mode, where the mode gate would otherwise return first', () => {
    const { runner, config } = makeRunner({ runQueryResult: { entries: sampleEntries(2), totalCount: 2 } })
    config.state.setResults(sampleEntries(2))
    config.state.setMode('ai')
    config.state.setQuery('')
    runner.scheduleSearch()
    expect(config.state.getResults()).toEqual([])
  })
})

describe('executeQuery', () => {
  it('writes results, total, cursor, and the ⏎ ownership event', async () => {
    const { runner, config } = makeRunner({ runQueryResult: { entries: sampleEntries(3), totalCount: 9 } })
    config.state.setCursorIndex(2)
    await runner.executeQuery()
    expect(config.state.getResults()).toHaveLength(3)
    expect(config.state.getTotalCount()).toBe(9)
    expect(config.state.getCursorIndex()).toBe(0)
    expect(config.state.getLastDialogEvent()).toBe('results-arrived')
    expect(config.state.getLastRunQuery()).toBe('report')
    expect(config.state.getIsSearching()).toBe(false)
    expect(runner.hasSearched).toBe(true)
  })

  it('drops a stale AI strip on a plain run, and keeps it on the AI path', async () => {
    const { runner, config } = makeRunner()
    config.state.setLastAiPrompt('big photos')
    config.state.setLastAiCaveat('Guessed the size')
    await runner.executeQuery(true)
    expect(config.state.getLastAiPrompt()).toBe('big photos')
    await runner.executeQuery()
    expect(config.state.getLastAiPrompt()).toBeNull()
    expect(config.state.getLastAiCaveat()).toBeNull()
  })

  it('bails before running when the index is not ready, and clears the spinner', async () => {
    const { runner, config, calls } = makeRunner({ overrides: { isIndexReady: false } })
    config.state.setIsSearching(true)
    await runner.executeQuery()
    expect(calls.runQuery).toBe(0)
    expect(config.state.getIsSearching()).toBe(false)
    expect(runner.hasSearched).toBe(true)
  })

  it('surfaces the backend reason instead of an empty list', async () => {
    const { runner, config } = makeRunner({ runQueryError: new Error('Query too broad. Add a filename pattern') })
    await runner.executeQuery()
    const messages = getToasts().map((t) => String(t.content))
    expect(messages.join(' ')).toContain('Query too broad')
    expect(config.state.getIsSearching()).toBe(false)
  })

  it('falls back to a generic reason when the rejection carries no message', async () => {
    const { runner } = makeRunner({ runQueryError: '   ' })
    await runner.executeQuery()
    expect(getToasts()).toHaveLength(1)
    expect(String(getToasts()[0].content)).not.toBe('')
  })
})

describe('scheduleSearch (auto-apply gates)', () => {
  it('runs once after the debounce, collapsing a burst of keystrokes', async () => {
    vi.useFakeTimers()
    const { runner, calls } = makeRunner()
    runner.scheduleSearch()
    runner.scheduleSearch()
    runner.scheduleSearch()
    expect(calls.runQuery).toBe(0)
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(1)
  })

  it('never auto-applies in AI mode (an AI run costs money)', async () => {
    vi.useFakeTimers()
    const { runner, config, calls } = makeRunner()
    config.state.setMode('ai')
    runner.scheduleSearch()
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(0)
  })

  it('stays quiet while `search.autoApply` is off', async () => {
    vi.useFakeTimers()
    const { runner, calls, setAutoApply } = makeRunner()
    setAutoApply(false)
    runner.scheduleSearch()
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(0)
  })

  it('stays quiet mid-IME-composition, then fires once on composition end', async () => {
    vi.useFakeTimers()
    const { runner, calls } = makeRunner()
    runner.handleCompositionStart()
    runner.scheduleSearch()
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(0)
    runner.handleCompositionEnd()
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(1)
  })

  it('dispose cancels a pending run so an unmounted dialog stays quiet', async () => {
    vi.useFakeTimers()
    const { runner, calls } = makeRunner()
    runner.scheduleSearch()
    runner.dispose()
    await vi.advanceTimersByTimeAsync(SEARCH_AUTO_APPLY_DEBOUNCE_MS)
    expect(calls.runQuery).toBe(0)
  })
})

describe('runAiSearch', () => {
  const translated: AiTranslateResult = { caveat: 'Guessed 10 MB as "big"', highlightedFields: ['size', 'query'] }

  it('captures the prompt before the IPC, then runs the query and scrolls to the top', async () => {
    const seen: (string | null)[] = []
    const { runner, config, calls } = makeRunner({
      query: 'big photos',
      translateAi: (prompt) => {
        // The prompt has to be readable even if the IPC never comes back.
        seen.push(config.state.getLastAiPrompt())
        expect(prompt).toBe('big photos')
        return Promise.resolve(translated)
      },
    })
    await runner.runAiSearch('  big photos  ')
    expect(seen).toEqual(['big photos'])
    expect(config.state.getLastAiCaveat()).toBe('Guessed 10 MB as "big"')
    expect(calls.runQuery).toBe(1)
    expect(calls.scrollCursorIntoView).toBe(1)
    expect(config.state.getIsSearching()).toBe(false)
  })

  it('flashes the fields the AI touched, then clears them', async () => {
    vi.useFakeTimers()
    const { runner } = makeRunner({ query: 'big photos', translateAi: () => Promise.resolve(translated) })
    const done = runner.runAiSearch('big photos')
    await vi.advanceTimersByTimeAsync(0)
    await done
    expect([...runner.highlightedFields]).toEqual(['size', 'query'])
    await vi.advanceTimersByTimeAsync(AI_HIGHLIGHT_FLASH_MS)
    expect([...runner.highlightedFields]).toEqual([])
  })

  it('keeps one `SvelteSet` instance across runs so readers stay subscribed', async () => {
    const { runner } = makeRunner({ query: 'big photos', translateAi: () => Promise.resolve(translated) })
    const first = runner.highlightedFields
    await runner.runAiSearch('big photos')
    expect(runner.highlightedFields).toBe(first)
    await runner.runAiSearch('big photos')
    // Pre-fix a fresh set landed here on every run, and the AI flash stopped repainting.
    expect(runner.highlightedFields).toBe(first)
  })

  it('does nothing for a blank prompt or a consumer with no AI', async () => {
    const { runner, calls } = makeRunner({ query: 'big photos', translateAi: () => Promise.resolve(translated) })
    await runner.runAiSearch('   ')
    expect(calls.translateAi).toEqual([])

    const noAi = makeRunner({ query: 'big photos' })
    await noAi.runner.runAiSearch('big photos')
    expect(noAi.calls.runQuery).toBe(0)
  })

  it('toasts a translation failure and clears the spinner', async () => {
    const { runner, config, calls } = makeRunner({
      query: 'big photos',
      translateAi: () => Promise.reject(new Error('boom')),
    })
    await runner.runAiSearch('big photos')
    expect(getToasts()).toHaveLength(1)
    expect(config.state.getIsSearching()).toBe(false)
    expect(calls.runQuery).toBe(0)
    // The prompt stays on screen so the user sees what they asked.
    expect(config.state.getLastAiPrompt()).toBe('big photos')
  })

  it('treats a null translation as a benign no-op, spinner off', async () => {
    const { runner, config, calls } = makeRunner({ query: 'big photos', translateAi: () => Promise.resolve(null) })
    await runner.runAiSearch('big photos')
    expect(getToasts()).toHaveLength(0)
    expect(config.state.getIsSearching()).toBe(false)
    expect(calls.runQuery).toBe(0)
  })
})

describe('run / runFromButton', () => {
  it('runs the plain query in a non-AI mode', () => {
    const { runner, calls } = makeRunner()
    runner.runFromButton()
    expect(calls.runQuery).toBe(1)
  })

  it('routes to the AI round-trip in AI mode', async () => {
    const { runner, config, calls } = makeRunner({
      query: 'big photos',
      translateAi: () => Promise.resolve({ caveat: null }),
    })
    config.state.setMode('ai')
    runner.runFromButton()
    await vi.waitFor(() => {
      expect(calls.translateAi).toEqual(['big photos'])
    })
  })

  it('the button path respects disabled inputs; the ⏎ path does not', () => {
    const { runner, calls } = makeRunner({ overrides: { inputsDisabled: true } })
    runner.runFromButton()
    expect(calls.runQuery).toBe(0)
    runner.run()
    expect(calls.runQuery).toBe(1)
  })

  it('runAiFromQuery stays quiet when AI is off or the query is blank', async () => {
    const offConfig = makeRunner({
      query: 'big photos',
      translateAi: () => Promise.resolve({ caveat: null }),
      overrides: { aiEnabled: false },
    })
    offConfig.runner.runAiFromQuery()
    await Promise.resolve()
    expect(offConfig.calls.translateAi).toEqual([])

    const blank = makeRunner({ query: '', translateAi: () => Promise.resolve({ caveat: null }) })
    blank.runner.runAiFromQuery()
    await Promise.resolve()
    expect(blank.calls.translateAi).toEqual([])
  })
})

describe('hasSearched', () => {
  it('is off until a run, and resettable for the ⌘N / prefill paths', async () => {
    const { runner } = makeRunner()
    expect(runner.hasSearched).toBe(false)
    await runner.executeQuery()
    expect(runner.hasSearched).toBe(true)
    runner.setHasSearched(false)
    expect(runner.hasSearched).toBe(false)
  })
})
