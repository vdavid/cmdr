/**
 * Builder for a favorite row's hover tooltip in the volume switcher. Kept
 * out of `VolumeBreadcrumb.svelte` so the path-first ordering and the
 * platform-forked reorder hint are unit-testable without a DOM.
 */

import { tString } from '$lib/intl/messages.svelte'

/**
 * Tooltip for a favorite row. Leads with the PATH so a renamed favorite
 * ("Documents" → "Docs") still reveals where it points, then the
 * keyboard-reorder + context hints. The global tooltip CSS renders with
 * `white-space: pre-line`, so the `\n` becomes a real line break.
 *
 * macOS has no Alt key, so the reorder hint reads `⌥↑ / ⌥↓` (Option symbol +
 * arrow glyphs); other platforms spell out `Alt+↑ / Alt+↓`.
 */
export function buildFavoriteTooltip(path: string, isMac: boolean): string {
  const reorder = isMac ? '⌥↑ / ⌥↓' : 'Alt+↑ / Alt+↓'
  return tString('fileExplorer.navigation.favoriteTooltip', { path, reorder })
}
