/**
 * The words and digits in the native file context menu's header line.
 *
 * The header names what the menu is about to act on: `photo.jpg · 2.1 MB` for one row,
 * `3 items · 3.2 MB` when the right-click landed inside a selection. Rust decides which
 * of those two shapes to draw — it owns the only fact that settles it, how many paths
 * the menu will act on — and this module supplies the text that goes in.
 *
 * ❗ Both halves are rendered HERE rather than in Rust, for one reason: locale-aware
 * number formatting. A size depends on `appearance.fileSizeFormat` and
 * `listing.sizeUnit` and only `$lib/units/byte-size.ts` honours them; a count needs the
 * active locale's grouping separator (`12,345` / `12.345`) and its plural category,
 * which `$lib/intl` has and `crate::intl`'s `menu_t` deliberately does not (it's a table
 * lookup with no ICU and no number formatting). Composing either in Rust would be a
 * second, worse implementation drifting from the pane's own status bar and size column,
 * with both on screen at once. See `src-tauri/src/menu/context_menu_header.rs`.
 *
 * ❌ No fallback number for a size that isn't honest: the header shows the name alone
 * rather than a zero the user would read as "empty".
 */

import { formatSizeText } from './selection-info-utils'
import { getDisplaySize } from '../views/full-list-utils'
import type { ListingStats } from '../types'
import { formatInteger } from '$lib/intl/number-format'
import { tString } from '$lib/intl/messages.svelte'
import { getFileSizeFormat, getFileSizeUnit, getSizeDisplayMode } from '$lib/settings/reactive-settings.svelte'

/** What the size rule needs off a row. Both file panes' rows and search hits fit it. */
export interface SizedRow {
  isDirectory: boolean
  /**
   * Absent when the backend has no size for this row yet. Optional AND nullable,
   * because `FileEntry.size` is optional while the search backend's is a serialized
   * `Option` (`null`).
   */
  size?: number | null
}

/**
 * The byte count the header should show for these rows, or `null` for "show none".
 *
 * `null` whenever the answer wouldn't be honest:
 * - **no rows** — nothing to total;
 * - **any folder among them** — a folder's size is its subtree's, which the index may
 *   still be walking; the pane itself shows `<dir>` rather than a number for exactly
 *   this reason, and a header that guessed would contradict it;
 * - **any row with no size yet** — a partial sum understates the pile silently.
 */
export function contextMenuSizeBytes(rows: readonly SizedRow[]): number | null {
  if (rows.length === 0) return null
  let total = 0
  for (const row of rows) {
    if (row.isDirectory || row.size == null) return null
    total += row.size
  }
  return total
}

/**
 * The header's total for a right-click that landed INSIDE the pane's selection, read
 * off the same listing stats the status bar shows, or `null` for "show none".
 *
 * A pane can't hand {@link contextMenuSizeBytes} its rows (a 500k-row listing lives
 * outside reactivity by design), so it comes through the backend's totals instead —
 * with the same folder rule: any selected folder and there's no honest number, because
 * its recursive size may still be settling. Honours `listing.sizeDisplayMode`, so the
 * header agrees with the selection summary beside it.
 */
export function contextMenuSelectionSizeBytes(stats: ListingStats | null): number | null {
  if (!stats || stats.selectedSize == null || (stats.selectedDirs ?? 0) > 0) return null
  return getDisplaySize(stats.selectedSize, stats.selectedPhysicalSize, getSizeDisplayMode()) ?? stats.selectedSize
}

/**
 * `bytes` as the text the header carries, or `undefined` for "no size to show" —
 * which is exactly what `showFileContextMenu`'s `target.sizeText` means when omitted.
 *
 * Reads the user's two size settings, so it renders exactly what the pane's size
 * column renders for the same number.
 */
export function contextMenuSizeText(bytes: number | null): string | undefined {
  if (bytes == null) return undefined
  return formatSizeText(bytes, { unit: getFileSizeUnit(), format: getFileSizeFormat() })
}

/**
 * How many rows the menu will act on, worded for the active language (`3 items`,
 * `12 345 objekt`), or `undefined` when the header won't use a count.
 *
 * `undefined` below two, because one row shows its filename instead — that's Rust's
 * rule and this mirrors it so a stray string never crosses. The count itself still
 * travels as `paths.length`, and the backend re-derives its own; this is only the
 * wording, which needs the locale's grouping separator and plural form.
 */
export function contextMenuCountText(count: number): string | undefined {
  if (count < 2) return undefined
  return tString('fileExplorer.contextMenu.itemCount', { count, countText: formatInteger(count) })
}
