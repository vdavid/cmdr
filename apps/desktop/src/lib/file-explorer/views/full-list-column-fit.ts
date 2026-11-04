/**
 * Which optional Full-view columns fit the pane, Finder-style: as the pane
 * narrows, the date column drops first, then Ext, so Name keeps a readable
 * width. Icon and Size always stay. Pure, so `FullList` and its tests share one
 * answer; the rationale and the threshold numbers are in `DETAILS.md` § "Narrow
 * panes drop the date, then Ext".
 */

/**
 * The Name column's floor at 100 % text size: about 18 average characters, room
 * for a name to read before middle-truncation eats it. Scales with text size,
 * like every other track.
 */
export const FULL_NAME_MIN_WIDTH = 120

/**
 * A row's horizontal space outside its grid tracks: `.file-entry`'s
 * `--spacing-sm` padding plus `.listbox-region`'s `--spacing-xs` gutter, both
 * sides. Keep in sync with those rules in `FullList.svelte`.
 */
const ROW_INLINE_CHROME = 24

/** `.file-entry`'s `gap` (`--spacing-sm`) between grid tracks. */
const TRACK_GAP = 8

export interface FullListColumnFit {
  showDate: boolean
  /** `false` when the Ext column has to go; its extension then rides in Name. */
  showExt: boolean
}

export function fitFullListColumns(args: {
  /** The scroll surface's `clientWidth` (scrollbar already excluded); 0 while unmeasured. */
  rowAreaWidth: number
  iconWidth: number
  /** The Git track's width, or 0 when there's no Git column. */
  gitWidth: number
  /**
   * The measured tracks. Measure them independently of this fit (with the
   * `showExtensionInName` SETTING, not the fitted layout), or dropping Ext
   * returns `ext: 0`, which makes it fit again, and the column flaps.
   */
  widths: { ext: number; size: number; date: number }
  /** The effective text-size scale (system × user). */
  scale: number
}): FullListColumnFit {
  const { rowAreaWidth, iconWidth, gitWidth, widths, scale } = args
  // Unmeasured (first paint, or a layout-less environment): today's full layout.
  if (rowAreaWidth <= 0) return { showDate: true, showExt: true }

  const nameMin = FULL_NAME_MIN_WIDTH * scale
  const fixed = [iconWidth, widths.size, ...(gitWidth > 0 ? [gitWidth] : [])]
  const ext = widths.ext > 0 ? [widths.ext] : []
  // Every non-Name track costs its width plus one gap.
  const nameWidth = (tracks: number[]): number =>
    rowAreaWidth - ROW_INLINE_CHROME - tracks.reduce((sum, w) => sum + w + TRACK_GAP, 0)

  if (nameWidth([...fixed, ...ext, widths.date]) >= nameMin) return { showDate: true, showExt: true }
  if (ext.length === 0 || nameWidth([...fixed, ...ext]) >= nameMin) return { showDate: false, showExt: true }
  return { showDate: false, showExt: false }
}
