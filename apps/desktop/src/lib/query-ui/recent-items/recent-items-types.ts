// Shared types for the generic recent-items footer + popover. Both components are generic
// over the consumer's entry shape `E`; the adapter is the only seam where consumer-specific
// fields (Search's `excludeSystemDirs`, Selection's narrower entry, etc.) leak in.
//
// Search wires the adapter to its `HistoryEntry`; Selection (M7+) wires its own. The
// `recent-chips-layout.ts` packer only sees the adapted `{ label, tooltip }`, so packing is
// identical across consumers.

import type { HistoryMode } from '$lib/tauri-commands'

/**
 * Shape produced by the consumer's adapter and consumed by the chip / row UI. Kept narrow
 * on purpose so the components themselves never depend on the entry's internals.
 */
export interface RecentItemView {
  /** Primary chip text (typically the query, possibly truncated for the cell). */
  label: string
  /** Multi-line plain-text tooltip shown on hover. */
  tooltip: string
  /** Drives the mode badge (`AI` / `Aa` / `.*`) on the chip. */
  mode: HistoryMode
  /** Short relative age string (`just now`, `5m ago`); shown in row layouts. */
  ageLabel: string
  /** Full accessible name for AT (typically prefixed with "Run recent search: …"). */
  ariaLabel: string
}

/**
 * Adapter callback turning a consumer-specific entry into the view shape. Pure; called per
 * render. Keep it cheap: the components call it once per visible chip per render pass.
 */
export type RecentItemAdapter<E> = (entry: E) => RecentItemView

/**
 * Stable identity for an entry. The Svelte `{#each (key)}` blocks key against this, so it
 * MUST be stable across renders. Search uses `entry.id` (the history-store UUID); Selection
 * will do the same.
 */
export type RecentItemKey<E> = (entry: E) => string
