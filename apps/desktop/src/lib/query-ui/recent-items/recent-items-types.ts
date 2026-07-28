// Shared types for the generic recent-items dropdown. The popover is generic over the
// consumer's entry shape `E`; the adapter is the only seam where consumer-specific fields
// (Search's `excludeSystemDirs`, Selection's narrower entry, etc.) leak in.
//
// Search wires the adapter to its `HistoryEntry`; Selection wires its own.

import type { HistoryMode } from '$lib/tauri-commands'

/**
 * Shape produced by the consumer's adapter and consumed by the row UI. Kept narrow on
 * purpose so the component itself never depends on the entry's internals.
 */
export interface RecentItemView {
  /** Primary row text: the query as the user typed it. */
  label: string
  /** Multi-line plain-text tooltip shown on hover. */
  tooltip: string
  /** Drives the mode badge (`AI` / `Aa` / `.*`) on the row. */
  mode: HistoryMode
  /** Short relative age string (`just now`, `5m ago`); leads the row's meta line. */
  ageLabel: string
  /**
   * The rest of the row's meta line, already joined: result count then filter summary
   * (`12 results · size > 1 MB, case-sensitive`). Empty string when there's nothing to say,
   * in which case the row shows the age alone. Built by `rowMeta()`.
   */
  metaLabel: string
  /** Full accessible name for AT (typically prefixed with "Run recent search: …"). */
  ariaLabel: string
}

/**
 * Adapter callback turning a consumer-specific entry into the view shape. Pure; called per
 * render. Keep it cheap: the popover calls it once per entry per `entries` change.
 */
export type RecentItemAdapter<E> = (entry: E) => RecentItemView

/**
 * Stable identity for an entry. The Svelte `{#each (key)}` blocks key against this, so it
 * MUST be stable across renders. Search uses `entry.id` (the history-store UUID); Selection
 * will do the same.
 */
export type RecentItemKey<E> = (entry: E) => string
