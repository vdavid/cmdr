/**
 * Config shape consumed by `QueryDialog.svelte`.
 *
 * The orchestrator is a shared primitive between Search and Selection. Each consumer
 * wires its own data source, AI translation, history store, primary/secondary actions,
 * and lifecycle hooks via this config.
 *
 * Everything that diverges per consumer lives here; everything else (overlay layout,
 * keyboard dispatch, IME guard, auto-apply debounce, `lastDialogEvent` ownership,
 * Enter ownership swap via `deriveEnterAction`, title bar, mode chips, filter chips,
 * results list, the query field's recent-items dropdown, empty state, notice banner) lives
 * in `QueryDialog.svelte` and is the same code for every consumer.
 *
 * Ownership contracts the consumer MUST NOT violate:
 *
 *   1. `state.lastDialogEvent` is QueryDialog's. Don't write to it from `runQuery`,
 *      `translateAi`, or any handler. QueryDialog writes 'opened' on mount,
 *      'query-edited' on bar input, 'filter-edited' on FilterChips writes,
 *      'cursor-moved' on ↑/↓ and hover, and 'results-arrived' after `runQuery`
 *      resolves. Writing it from a callback breaks the ⏎ ownership swap.
 *
 *   2. `state.lastAiPrompt` and `state.lastAiCaveat` are also QueryDialog's. The
 *      dialog sets the prompt to the trimmed user input BEFORE invoking
 *      `translateAi`; it sets the caveat to whatever `translateAi` returns.
 *      Don't mutate either from a callback.
 *
 *   3. The consumer's `translateAi` callback owns applying every other AI-returned
 *      field to state (size, date, scope, caseSensitive, AI pattern + label, etc).
 *      It returns `{ caveat, highlightedFields }`; QueryDialog flashes the listed
 *      fields and writes the caveat after the promise resolves.
 *
 *   4. The consumer's `runQuery` returns `{ entries, totalCount }` and does NOT
 *      write to `state.results` / `state.totalCount` / `state.cursorIndex` /
 *      `state.lastDialogEvent`. QueryDialog handles all of those.
 */

import type { BadgeStatus } from '$lib/feature-status'
import type { SearchResultEntry } from '$lib/tauri-commands'
import type { SoftDialogId } from '$lib/ui/dialog-registry'
import type { QueryFilterState, SearchMode } from './query-filter-state.svelte'
import type { QueryStreamSource } from './query-stream'
import type { RecentItemAdapter, RecentItemKey } from './recent-items/recent-items-types'
import type { RecentItemsStore } from './recent-items/recent-items-state.svelte'

/** Which filter chips render in the strip. Search shows all four; Selection hides scope. */
export interface QueryDialogVisibleChips {
  size: boolean
  date: boolean
  scope: boolean
  pattern: boolean
}

/** One example chip in the dialog's empty state. */
export interface QueryDialogEmptyExample {
  label: string
  mode: SearchMode
  query: string
}

/** Empty-state content. Both consumers show three examples; only Search shows the index hint. */
export interface QueryDialogEmptyState {
  examples: QueryDialogEmptyExample[]
  /** Search shows "Index ready: N entries"; Selection omits. */
  indexEntryCount?: number
  /** Search shows the keyboard tip; Selection has its own copy. */
  keyboardHint?: string
}

/**
 * Result of an AI translation. The consumer's `translateAi` callback applies the
 * AI's filter writes itself; QueryDialog only consumes the metadata it needs to
 * surface in the AI transparency strip and the flash effect.
 */
export interface AiTranslateResult {
  caveat: string | null
  /** Names of filter chips to briefly highlight (for example 'size', 'date', 'pattern'). */
  highlightedFields?: string[]
}

/**
 * The two rungs of the scope ladder the Search-in popover offers. A search covers at
 * most one volume, so there are exactly two: the focused pane's current folder (the
 * default) and the volume it lives on (the maximum). Resolved by
 * `$lib/search/searchable-folder`.
 */
export interface ScopePresets {
  /** "Use current folder" (⌥C), or `null` when the focused pane is a snapshot with no real folder behind it. */
  currentFolder: string | null
  /** Why the current folder is unavailable, for the disabled button's tooltip. `''` when it is available. */
  currentFolderUnavailableReason: string
  /** "This volume" (⌥V): the mount root of the volume the current folder lives on. */
  volumeRoot: string
}

/**
 * Search-specific filter-chips state that QueryDialog forwards to `FilterChips.svelte`.
 *
 * Selection passes empty/no-op values for the Search-only fields with
 * `scopeChipVisible: false` and a Pattern-chip surface that doesn't require them.
 * Keeping the props named the way the underlying component speaks means
 * `FilterChips.svelte`'s prop list stays stable for both consumers.
 */
export interface QueryDialogFilterChipsExtras {
  caseSensitive: boolean
  scope: string
  excludeSystemDirs: boolean
  /**
   * The two scope presets the Search-in popover offers ("Use current folder" / "This
   * volume") plus the default an unset scope resolves to. Built by the dialog's host
   * from the focused pane; see `$lib/search/searchable-folder`.
   */
  scopePresets: ScopePresets
  /**
   * What an EMPTY scope box means: the path the search actually runs against, and the
   * name the chip and placeholder show for it. Search derives it from `scopePresets`
   * (`resolveDefaultScope`); Selection, which hides the scope chip, passes blanks.
   */
  defaultScope: { path: string; label: string }
  systemDirExcludeTooltip: string
  aiPattern: string | null
  /**
   * Kind of the AI-produced pattern (`'glob'` / `'regex'` / null). The transparency strip uses
   * it to label the pattern row precisely. Search wires its `lastAiPatternKind`; Selection passes
   * `null` (its `translateAi` clears the other-kind buffer, so the strip's handTyped fallback is
   * already kind-correct there).
   */
  aiPatternKind: 'glob' | 'regex' | null
  onToggleCaseSensitive: () => void
  onToggleExcludeSystemDirs: () => void
  onSetScope: (path: string) => void
  onClearAiPattern: () => void
  /**
   * Count-only mode: current state. Search wires it; Selection omits it (its results
   * are always a listable set, so a bare count has no meaning there). When
   * `onToggleCountOnly` is undefined the toggle doesn't render.
   */
  countOnly?: boolean
  /** Toggles count-only mode. Presence gates the toggle's render (Search-only). */
  onToggleCountOnly?: () => void
}

/** Optional action button: primary (⌥⏎ in Search), secondary (⏎ in Search). */
export interface QueryDialogAction {
  /** Button label, e.g. "Show all in main window" or "Select these files". */
  label: string
  /** Inline shortcut hint, e.g. "⌥⏎" or "⏎". */
  shortcutHint: string
  /** Variant for the underlying Button component. Defaults to 'primary' for the primary slot. */
  variant?: 'primary' | 'secondary'
  /** Tooltip text shown on hover. */
  tooltip?: string
  /** ARIA label. Defaults to `label`. */
  ariaLabel?: string
}

/** Primary action handler: invoked on ⌥⏎ (Search) or ⏎ (Selection). Receives the current entries. */
export interface QueryDialogPrimaryAction extends QueryDialogAction {
  handler: (entries: SearchResultEntry[]) => void | Promise<void>
}

/** Secondary action handler: invoked on ⏎ when `deriveEnterAction === 'go-to-file'`. */
export interface QueryDialogSecondaryAction extends QueryDialogAction {
  handler: (entry: SearchResultEntry) => void | Promise<void>
}

/** Generic history entry the query field's recent-items dropdown renders. */
export interface QueryDialogRecentItems<E> {
  /** Adapts a history entry into the row UI's shape. */
  adapter: RecentItemAdapter<E>
  /** Stable identity for keying. */
  keyFn: RecentItemKey<E>
  /** ARIA label on the query field's dropdown-trigger chevron (default "All recent searches"). */
  triggerAriaLabel?: string
  /** Tooltip on the dropdown-trigger chevron (default "Show all recent searches"). */
  triggerTooltip?: string
  /** Filter input placeholder in the dropdown. */
  filterPlaceholder?: string
  /** Empty-message in the dropdown when the filter has no matches. */
  emptyMessage?: string
  /** ARIA label for the dropdown wrapper. */
  popoverAriaLabel?: string
  /** ARIA label for the listbox inside the dropdown. */
  listboxAriaLabel?: string
}

/**
 * The shape every consumer of `QueryDialog` builds.
 *
 * Generic over `E` (the history entry type): Search wires `HistoryEntry`, Selection
 * wires `SelectionHistoryEntry`.
 */
export interface QueryDialogConfig<E = unknown> {
  /** Dialog title shown in the title bar. */
  title: string
  /**
   * Optional stability badge rendered next to the title (uppercase ALPHA / BETA
   * pill). Derive it from `getBadgeStatus(id)` in `$lib/feature-status` so the
   * repo-root `feature-status.json` stays the single source of truth.
   */
  badge?: BadgeStatus
  /**
   * Registered dialog id passed to `notifyDialogOpened` / `notifyDialogClosed` and
   * to the close registry. Typed against `SOFT_DIALOG_REGISTRY` so a consumer can't
   * ship a dialog MCP doesn't know about.
   */
  dialogType: SoftDialogId
  /**
   * The width the dialog OPENS at, e.g. `'min(1080px, 80vw)'`. Not a cap: the panel is
   * resizable, so the user can drag it wider (up to the viewport).
   */
  width: string

  /** Cross-consumer state instance (the core factory's output). */
  state: QueryFilterState

  /** Whether the AI mode chip is available + AI-mode workflows are wired. */
  aiEnabled: boolean
  /** True when inputs/filters should render disabled (e.g. Search's index not ready). */
  inputsDisabled: boolean

  /** Per-chip visibility. */
  visibleChips: QueryDialogVisibleChips
  /** Whether the results table shows the Path column. */
  showPathColumn: boolean

  /**
   * Copy for the QueryBar's right-gutter run hint. Each dialog names its own verb
   * ("Press Enter to search" / "Press Enter to filter"). Omit it to take the shared
   * `queryUi.bar.runHint` default.
   */
  runHintCopy?: string
  /**
   * Overrides the run button's tooltip and accessible name (filename / regex modes; AI
   * mode keeps its own). Search sets it so the button VOICES what Enter does that
   * auto-apply won't: look through folders that aren't indexed yet (Decision 7).
   */
  runTitleOverride?: string

  /** Recent-items store. */
  historyStore: RecentItemsStore<E>
  /** Recent-items dropdown adapter + copy. */
  recentItems: QueryDialogRecentItems<E>
  /** Loads up the history list on mount. Idempotent. */
  onLoadHistory?: () => void | Promise<void>

  /** Empty-state config. */
  emptyState: QueryDialogEmptyState

  /** Search-specific filter-chips state. Selectionpasses a narrower shape. */
  filterChipsExtras: QueryDialogFilterChipsExtras

  /** Scan progress for the "Drive index not ready" state. Search only. */
  scanning: boolean
  entriesScanned: number
  /** Whole-drive entry count (Search). Selection passes 0. */
  indexEntryCount: number
  /** Drive index availability (Search). Selection passes `true` (Selection has no index). */
  isIndexAvailable: boolean
  isIndexReady: boolean

  /**
   * Optional notice banner shown below the AI strip and above the filter chips.
   * Selection uses it on snapshot panes ("Matching what's shown…"); Search passes
   * `undefined`. The banner is purely informational; clicking does nothing.
   * Empty/undefined hides the row.
   */
  noticeBanner?: string

  /**
   * Optional consumer-owned banner rendered directly ABOVE the results table, where a
   * caveat about the answer belongs. Search uses it for the coverage note (which
   * scopes the run couldn't cover, and what to do about it); other consumers pass
   * `undefined` and the row doesn't render. The mirror of `resultsExtra`: the snippet
   * owns its own state, and QueryDialog only gives it a slot.
   */
  resultsNotice?: import('svelte').Snippet

  /**
   * Optional consumer-owned section rendered directly BELOW the main results table
   * (and above the footer). Search uses it for the "text in images" OCR results grid
   * (a distinct result type with its own thumbnails + coverage-honesty states); other
   * consumers pass `undefined` and the row doesn't render. The snippet owns its own
   * data fetching + lifecycle; QueryDialog only gives it a slot, so it never touches
   * the shared `results` / `cursorIndex` contract.
   */
  resultsExtra?: import('svelte').Snippet

  /**
   * Executes the query in the consumer's data source. Receives nothing; reads
   * what it needs off `state`. Returns the result set. QueryDialog handles
   * writing `state.results` / `state.totalCount` / `state.cursorIndex` and
   * `state.lastDialogEvent = 'results-arrived'`. Do NOT write any of those from
   * inside `runQuery`.
   */
  runQuery: () => Promise<{ entries: SearchResultEntry[]; totalCount: number }>

  /**
   * Optional transport for a query that answers over TIME rather than in one promise.
   * Search wires it (a search of ground the index doesn't cover walks it, so rows
   * arrive in batches over seconds or minutes); Selection matches a pane listing it
   * already holds, so it leaves this undefined and nothing about streaming reaches it.
   *
   * The runner owns the run — the id, the generation guard, appending, the cursor,
   * the completion re-rank — and this owns only the wire. Contract and vocabulary:
   * `query-stream.ts`. Runs the USER asked for take this path; auto-applied ones take
   * `runQuery` (Decision 7).
   */
  streamingSource?: QueryStreamSource

  /**
   * Optional AI translation. The consumer's callback applies AI-returned filter
   * writes (size, date, scope, AI pattern + label, …) and returns the caveat +
   * which fields to flash. QueryDialog handles capturing the prompt
   * (`state.lastAiPrompt`) before invoking and setting the caveat
   * (`state.lastAiCaveat`) after. Returns `null` on translation failure (silent).
   */
  translateAi?: (prompt: string) => Promise<AiTranslateResult | null>

  /** Primary action (Search's "Show all in main window"). */
  primaryAction?: QueryDialogPrimaryAction
  /** Secondary action (Search's "Go to file"). */
  secondaryAction?: QueryDialogSecondaryAction

  /** Called when a path-pill ancestor segment is clicked. */
  onPickPath: (ancestorPath: string) => void
  /** Called when the user picks an example chip in the empty state. */
  onPickExample: (chip: { mode: SearchMode; query: string }) => void
  /** Called when the user opens the row's `…` menu (or right-clicks the row). */
  onRowMenu: (entry: SearchResultEntry) => void
  /**
   * Called when the user picks a recent entry from the dropdown (Enter or click). LOADS the
   * entry into the consumer's state and nothing more: QueryDialog closes the dropdown, hands
   * `⏎` back to "run-search", and returns focus to the query field. Do NOT set `runOnMount`
   * from here — picking a past search is navigation, not a run, and an AI entry that ran
   * itself would spend the user's money on a keystroke.
   */
  onActivateRecent: (entry: E) => void
  /** Called when the user removes a recent entry (right-click on a dropdown row). */
  onRemoveRecent: (entry: E) => void

  /** Called on overlay click or Escape. */
  onClose: () => void

  /** Optional lifecycle hooks. */
  onMount?: () => void | Promise<void>
  onDestroy?: () => void

  /**
   * ⌘N hook: clears all consumer state ("new search" / "new selection"). When omitted,
   * QueryDialog falls back to `state.clearCore()` (the cross-consumer reset). Search's
   * wrapper supplies its `clearSearchState()` facade which also resets the Search extras
   * (scope, AI label/pattern, etc); Selection's wrapper can omit this and rely on the
   * core reset since it has no extras module.
   */
  onClearState?: () => void
}
