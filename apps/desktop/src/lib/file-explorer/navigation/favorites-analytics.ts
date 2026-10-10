/**
 * The payoff half of the favorites story.
 *
 * `favorite_changed` (backend, `favorites/store.rs`) counts the list being
 * edited; it cannot say whether anybody ever GOES anywhere with it, and a
 * favorites list nobody navigates from is a feature that only looks used. This
 * is the other half.
 *
 * PII-free: a favorite is a path plus a label the user typed, and neither
 * crosses. Only which surface it was picked from and how.
 */

import { trackEvent } from '$lib/tauri-commands'
import type { FavoriteReach } from '$lib/ipc/bindings'

/** How a favorite ROW was picked: its digit, its letter, Enter, or a click. */
export type FavoriteRowVia = 'digit' | 'letter' | 'keyboard' | 'pointer'

/**
 * A pick, as the event carries it: WHERE it happened and HOW.
 *
 * `surface` and `via` are ONE union rather than two independent enums, so
 * `surface: 'command'` can only ever pair with `via: 'command'` — the palette,
 * the Go menu, and the MCP `select_volume` tool are all arms where the command
 * IS the whole interaction, with no row to point at and so no `digit` /
 * `keyboard` / `pointer` answer. Said as a type, a dashboard splitting
 * `favorite_opened` by `via` can't meet a row where the prop went missing.
 *
 * `digit` is the question the menu's number column exists to answer: do the
 * number keys earn it, or does everyone arrow down anyway? `switcher` is the
 * volume switcher's favorites section, which numbers nothing, so it never
 * reports `digit`.
 */
export type FavoriteOpenedEvent =
  | { surface: 'favorites_menu' | 'switcher'; via: FavoriteRowVia }
  | { surface: 'command'; via: 'command' }
  /** The Dock tile's right-click menu: like `command`, the menu item IS the interaction. */
  | { surface: 'dock'; via: 'dock' }

/**
 * Reports a favorite being picked, with whether the pane could follow it (`reach`:
 * `ready` and `connects` open; the rest leave the pane put with a toast). So the
 * event counts PICKS, and `reach` splits arrivals from offline places.
 *
 * ONE emit site: `navigation/open-favorite.ts`, the single way a favorite opens.
 * It's the lowest chokepoint there is — every caller folds onto
 * `navigate({ to: { selectVolume } })`, which by then holds the CONTAINING
 * volume's id and can no longer tell a favorite from a drive — so the payload
 * comes down from whichever surface made the pick.
 */
export function reportFavoriteOpened(event: FavoriteOpenedEvent, reach: FavoriteReach['kind']): void {
  void trackEvent('favorite_opened', { surface: event.surface, via: event.via, reach })
}

/**
 * What brought the favorites menu up: ⌃D, the Go menu item, or the palette (a
 * menu-bar accelerator and a palette pick both arrive as the command), whether
 * typed over the panes or inside the open volume switcher.
 */
export type FavoritesMenuOpenTrigger = 'command'

/**
 * Reports the favorites menu coming up, against which `favorite_opened` reads as
 * a hit rate.
 */
export function reportFavoritesMenuOpened({ trigger }: { trigger: FavoritesMenuOpenTrigger }): void {
  void trackEvent('favorites_menu_opened', { trigger })
}
