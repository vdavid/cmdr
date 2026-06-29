/**
 * Pure model + geometry for the Finder-tag dot cluster (`TagDots.svelte`).
 *
 * Kept free of Svelte/DOM so Vitest can exercise the overflow/cap/colour-mapping
 * logic and the cluster-width math directly. The width constants here are the
 * single source of truth for the FE render; the Brief-mode backend width calc
 * (`src-tauri/.../listing/brief_columns.rs`) mirrors them so a reserved column
 * leaves exactly enough room for the cluster (keep both in sync).
 */

import type { TagRef } from '$lib/ipc/bindings'

/** Diameter of a single dot (border included), in CSS px. */
export const TAG_DOT_SIZE = 10
/** Left-edge advance per overlapping slot (so dots overlap by `SIZE - OFFSET`). */
export const TAG_DOT_OVERLAP_OFFSET = 5
/** Extra width the `+N` chip claims over a plain dot (room for two digits). */
export const TAG_CHIP_EXTRA = 8
/** Gap between the filename and the first dot. */
export const TAG_CLUSTER_GAP = 5
/** Colored-tag count at or below which every tag gets its own dot. */
export const TAG_MAX_DOTS = 3
/** Dots shown before the `+N` chip once the count exceeds `TAG_MAX_DOTS`. */
export const TAG_DOTS_BEFORE_CHIP = 2

/**
 * Colour-index → token suffix. Index 0 is colourless (no dot) and absent here;
 * 1-7 map to the `--color-tag-*` tokens in `app.css`.
 */
const TAG_COLOR_TOKENS: Record<number, string> = {
  1: 'grey',
  2: 'green',
  3: 'purple',
  4: 'blue',
  5: 'yellow',
  6: 'red',
  7: 'orange',
}

/** A colored tag has an index in 1-7; index 0 (colourless) renders no dot. */
export function isColoredTag(tag: TagRef): boolean {
  return tag.color >= 1 && tag.color <= 7
}

/** CSS `var(--color-tag-*)` reference for a colour index, or `undefined` if out of range. */
export function tagColorVar(color: number): string | undefined {
  const token = TAG_COLOR_TOKENS[color]
  return token ? `var(--color-tag-${token})` : undefined
}

export interface TagDotsModel {
  /** Dots to render, leftmost first, each carrying its colour index (1-7). */
  dots: { color: number }[]
  /** When > 0, render a `+N` chip after the dots; `N` is this value. */
  overflowCount: number
  /** Comma-separated tag names (ALL tags, including colourless) for a11y/hover. */
  label: string
}

/**
 * Reduces a file's tags to the dot cluster: drops colourless tags, caps the
 * dot count, and computes the `+N` overflow. Up to `TAG_MAX_DOTS` colored tags
 * show that many dots; beyond that, `TAG_DOTS_BEFORE_CHIP` dots plus a chip
 * reading `+(count - TAG_DOTS_BEFORE_CHIP)`.
 */
export function tagDotsModel(tags: TagRef[] | undefined): TagDotsModel {
  const all = tags ?? []
  const colored = all.filter(isColoredTag)
  const label = all.map((t) => t.name).join(', ')

  if (colored.length <= TAG_MAX_DOTS) {
    return { dots: colored.map((t) => ({ color: t.color })), overflowCount: 0, label }
  }
  return {
    dots: colored.slice(0, TAG_DOTS_BEFORE_CHIP).map((t) => ({ color: t.color })),
    overflowCount: colored.length - TAG_DOTS_BEFORE_CHIP,
    label,
  }
}

/**
 * Pixel width the dot cluster reserves to the right of the filename, as a pure
 * function of the colored-tag count (gap + overlapping slots + optional chip).
 * Returns 0 when there are no colored tags. Mirrored by the Brief backend width
 * calc so reserved columns don't clip the cluster.
 */
export function tagClusterWidthPx(coloredCount: number): number {
  if (coloredCount <= 0) return 0
  const slots = Math.min(coloredCount, TAG_MAX_DOTS)
  const hasChip = coloredCount > TAG_MAX_DOTS
  const base = TAG_DOT_SIZE + (slots - 1) * TAG_DOT_OVERLAP_OFFSET + (hasChip ? TAG_CHIP_EXTRA : 0)
  return TAG_CLUSTER_GAP + base
}

/** Colored-tag count for an entry's tags (drives `tagClusterWidthPx`). */
export function coloredTagCount(tags: TagRef[] | undefined): number {
  return (tags ?? []).filter(isColoredTag).length
}
