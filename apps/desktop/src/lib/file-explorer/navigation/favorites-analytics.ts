/**
 * The payoff half of the favorites story.
 *
 * `favorite_changed` (backend, `favorites/store.rs`) counts the list being
 * edited; it cannot say whether anybody ever GOES anywhere with it, and a
 * favorites list nobody navigates from is a feature that only looks used. This
 * is the other half.
 *
 * PII-free: a favorite is a path plus a label the user typed, and neither
 * crosses. Only which surface it was picked from.
 */

import { trackEvent } from '$lib/tauri-commands'

/** Where the pick happened. */
export type FavoriteOpenSurface =
  /** The volume dropdown in the breadcrumb: the way nearly everyone gets there. */
  | 'breadcrumb'
  /** The palette's volume commands, or the MCP `select_volume` tool. */
  | 'command'

/**
 * Reports navigating to a favorite.
 *
 * Called from the two places that branch on `category === 'favorite'`
 * (`VolumeBreadcrumb.handleVolumeSelect` and
 * `pane/volume-selection.ts::selectVolumeByIndex`). There's no lower chokepoint:
 * both fold onto `navigate({ to: { selectVolume } })`, which by then holds the
 * CONTAINING volume's id and can no longer tell a favorite from a drive.
 */
export function reportFavoriteOpened(surface: FavoriteOpenSurface): void {
  void trackEvent('favorite_opened', { surface })
}
