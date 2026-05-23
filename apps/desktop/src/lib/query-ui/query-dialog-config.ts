/**
 * Config shape consumed by `QueryDialog.svelte`.
 *
 * M4: the orchestrator becomes a shared primitive between Search and the upcoming
 * Selection dialog. Each consumer wires its own data source, AI translation, history
 * store, primary/secondary actions, and lifecycle hooks via this config.
 *
 * Everything that diverges per consumer lives here; everything else (overlay layout,
 * keyboard dispatch, IME guard, auto-apply debounce, `lastDialogEvent` ownership,
 * Enter ownership swap via `deriveEnterAction`, title bar, mode chips, filter chips,
 * results list, recent-items footer + popover, empty state, notice banner) lives in
 * `QueryDialog.svelte` and is the same code for every consumer.
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

import type { SearchResultEntry } from '$lib/tauri-commands'
import type { QueryFilterState, SearchMode } from './query-filter-state.svelte'
import type { RecentItemAdapter, RecentItemKey } from './recent-items/recent-items-types'
import type { RecentItemsStore } from './recent-items/recent-items-state.svelte'

/** Which filter chips render in the strip. Search shows all four; Selection (M7+) hides scope. */
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
 * Search-specific filter-chips state that QueryDialog forwards to `FilterChips.svelte`.
 *
 * Selection (M7+) will pass empty/no-op values for the Search-only fields once
 * `scopeChipVisible: false` and the Pattern-chip surface stop requiring them.
 * Keeping the props named the same way the underlying component already speaks
 * means M4 doesn't churn `FilterChips.svelte`'s prop list.
 */
export interface QueryDialogFilterChipsExtras {
  caseSensitive: boolean
  scope: string
  excludeSystemDirs: boolean
  searchableFolder: { path: string | null; disabled: boolean; disabledReason: string }
  systemDirExcludeTooltip: string
  aiPattern: string | null
  onToggleCaseSensitive: () => void
  onToggleExcludeSystemDirs: () => void
  onSetScope: (path: string) => void
  onClearAiPattern: () => void
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

/** Generic history entry the recent-items footer / popover renders. */
export interface QueryDialogRecentItems<E> {
  /** Adapts a history entry into the row UI's shape. */
  adapter: RecentItemAdapter<E>
  /** Stable identity for keying. */
  keyFn: RecentItemKey<E>
  /** Strip-leading label (default Search-flavoured: "Recent searches:"). */
  leadingLabel?: string
  /** Trailing button label (default "All searches…"). */
  trailingLabel?: string
  /** Trailing button tooltip. */
  trailingTooltipText?: string
  /** Inline shortcut on the trailing button. */
  trailingShortcut?: string
  /** ARIA region label on the footer strip. */
  ariaRegionLabel?: string
  /** ARIA label on the "All …" button. */
  ariaAllButtonLabel?: string
  /** Filter input placeholder in the popover. */
  filterPlaceholder?: string
  /** Empty-message in the popover when the filter has no matches. */
  emptyMessage?: string
  /** ARIA label for the popover wrapper. */
  popoverAriaLabel?: string
  /** ARIA label for the listbox inside the popover. */
  listboxAriaLabel?: string
}

/**
 * The shape every consumer of `QueryDialog` builds.
 *
 * Generic over `E` (the history entry type) so Search wires `HistoryEntry` and Selection
 * (M7+) wires `SelectionHistoryEntry`.
 */
export interface QueryDialogConfig<E = unknown> {
  /** Dialog title shown in the new title bar (M4 § "Title bar"). */
  title: string
  /** Dialog-type string passed to `notifyDialogOpened` / `notifyDialogClosed`. */
  dialogType: string
  /** Dialog max-width, e.g. `'min(1080px, 80vw)'`. */
  maxWidth: string

  /** Cross-consumer state instance (the M2 core factory). */
  state: QueryFilterState

  /** Whether the AI mode chip is available + AI-mode workflows are wired. */
  aiEnabled: boolean
  /** True when inputs/filters should render disabled (e.g. Search's index not ready). */
  inputsDisabled: boolean

  /** Per-chip visibility. */
  visibleChips: QueryDialogVisibleChips
  /** Whether the results table shows the Path column. */
  showPathColumn: boolean

  /** Copy for the QueryBar's right-gutter run hint. Locked in M4 (G14). */
  runHintCopy: string

  /** Recent-items store. */
  historyStore: RecentItemsStore<E>
  /** Recent-items adapter + copy. */
  recentItems: QueryDialogRecentItems<E>
  /** Loads up the history list on mount. Idempotent. */
  onLoadHistory?: () => void | Promise<void>

  /** Empty-state config. */
  emptyState: QueryDialogEmptyState

  /** Search-specific filter-chips state. Selection (M7+) passes a narrower shape. */
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
   * Used by R7 mitigation for snapshot-pane Selection ("Matching what's shown…").
   * Search passes `undefined`. The banner is purely informational; clicking does
   * nothing. Empty/undefined hides the row.
   */
  noticeBanner?: string

  /**
   * Executes the query in the consumer's data source. Receives nothing; reads
   * what it needs off `state`. Returns the result set. QueryDialog handles
   * writing `state.results` / `state.totalCount` / `state.cursorIndex` and
   * `state.lastDialogEvent = 'results-arrived'`. Do NOT write any of those from
   * inside `runQuery`.
   */
  runQuery: () => Promise<{ entries: SearchResultEntry[]; totalCount: number }>

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
  /** Called when the user activates a recent entry (chip click or popover Enter). */
  onActivateRecent: (entry: E) => void
  /** Called when the user removes a recent entry (chip right-click or popover right-click). */
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
