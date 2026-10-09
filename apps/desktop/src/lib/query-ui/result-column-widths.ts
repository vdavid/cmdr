/**
 * Width declarations for the Search results table (icon | Name | Path | Size | Modified),
 * fed to `$lib/ui/ColumnList.svelte`, which owns the measuring and the split.
 *
 * Search: Size and Modified shrink-wrap to their widest cell; Name and Path share the rest by
 * max-min fairness, each measured off the row DATA (Path as its uncollapsed pill strip, so a
 * Path track sized to its demand never trips `PathPills`' collapse). Selection: one folder,
 * no Path column, so Name is the lone flex track beside fixed `10ch` / `16ch` and nothing is
 * measured. DETAILS.md § Column widths.
 */

import type { SearchResultEntry } from '$lib/tauri-commands'
import type { ColumnListColumn } from '$lib/ui/column-list-types'
import { sizeDisplayParts } from '$lib/ui/size-display'
import { formattedDate } from '$lib/settings/reactive-settings.svelte'
import { PILL_CHROME_PX, PILL_SEPARATOR_GAP_PX, splitPath, totalWidth } from './path-pills-layout'

/**
 * Floor on the Name track, in CSS pixels. A list of very short names shouldn't squeeze
 * the column down to a couple of glyphs, and a narrow dialog still leaves Name readable.
 */
export const NAME_COL_MIN_PX = 80

/** Floor on the Path track, in CSS pixels: room for at least `…` plus a short pill. */
export const PATH_COL_MIN_PX = 120

/** The icon column's fixed track, in CSS pixels. */
export const ICON_TRACK_PX = 24

/** One row's cell texts, exactly as the row renders them. */
export interface ResultRowTexts {
  name: string
  /** The Path cell's pill labels, uncollapsed (`splitPath(parentPath)`). */
  pathLabels: string[]
  /** The Size cell's text; empty when the entry has no size. */
  size: string
  /** The Modified cell's text; empty when the entry has no date. */
  modified: string
}

/** The texts `<Size>`, `<DateLabel>`, and `PathPills` render for an entry. */
export function resultRowTexts(entry: SearchResultEntry): ResultRowTexts {
  return {
    name: entry.name,
    pathLabels: splitPath(entry.parentPath).map((s) => s.label),
    size:
      entry.size == null
        ? ''
        : sizeDisplayParts(entry.size)
            .map((p) => p.value)
            .join(''),
    modified: formattedDate(entry.modifiedAt).text,
  }
}

type ColumnSizing = Pick<ColumnListColumn<SearchResultEntry>, 'width' | 'demand' | 'headerDemand'>

export function resultColumnWidths(
  showPathColumn: boolean,
  texts: (entry: SearchResultEntry) => ResultRowTexts = resultRowTexts,
): Record<'icon' | 'name' | 'path' | 'size' | 'modified', ColumnSizing> {
  const icon: ColumnSizing = { width: { kind: 'fixed', px: ICON_TRACK_PX } }
  const name: ColumnSizing = { width: { kind: 'share', minPx: NAME_COL_MIN_PX } }
  const path: ColumnSizing = { width: { kind: 'share', minPx: PATH_COL_MIN_PX } }
  if (!showPathColumn) {
    return {
      icon,
      name,
      path,
      size: { width: { kind: 'fixed', ch: 10 } },
      modified: { width: { kind: 'fixed', ch: 16 } },
    }
  }
  return {
    icon,
    name: { ...name, demand: ({ row, measure }) => measure.text(texts(row).name) },
    path: {
      ...path,
      demand: ({ row, measure }) =>
        totalWidth(texts(row).pathLabels, measure.text, PILL_CHROME_PX, measure.text('/') + PILL_SEPARATOR_GAP_PX),
      // The header label sits inset by half a pill's chrome to line up with the first pill's text.
      headerDemand: ({ label, measure }) => measure.text(label) + PILL_CHROME_PX / 2,
    },
    size: {
      width: { kind: 'fit', fallback: { kind: 'fixed', ch: 10 } },
      demand: ({ row, measure }) => measure.tabular(texts(row).size),
    },
    modified: {
      width: { kind: 'fit', fallback: { kind: 'fixed', ch: 16 } },
      demand: ({ row, measure }) => measure.tabular(texts(row).modified),
    },
  }
}
