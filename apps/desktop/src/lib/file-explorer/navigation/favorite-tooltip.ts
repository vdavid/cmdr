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
 *
 * `status` is why a dimmed favorite is dimmed (`favorite-reach.ts::favoriteReachStatus`),
 * on its own line right under the path.
 */
export function buildFavoriteTooltip(path: string, isMac: boolean, status: string | null = null): string {
  const reorder = isMac ? '⌥↑ / ⌥↓' : 'Alt+↑ / Alt+↓'
  return status
    ? tString('fileExplorer.navigation.favoriteTooltipWithStatus', { path, status, reorder })
    : tString('fileExplorer.navigation.favoriteTooltip', { path, reorder })
}
