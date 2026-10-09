/**
 * The ONE way a favorite opens.
 *
 * A favorite is a virtual row (`id: 'fav-<uuid>'`, `category: 'favorite'`) pointing
 * at a folder on a real volume. Rust's reach pass (`favorites/reach.rs`) already put
 * the answer on the row: `favoriteTarget` names the volume (`volumeId`, its live
 * `volumeRoot`, with the row's `path` rebased onto it) and whether a pick gets there
 * (`reach`). So opening one reads the row; ❌ it never asks which volume contains the
 * path, because for an unmounted share the mount table walks UP and answers the boot
 * disk.
 *
 * - **`ready`, `connects`**: switch the pane onto the target at the favorite's path,
 *   `exact` (a favorite at a volume ROOT opens at the root). For `connects` the pane
 *   lands on a saved place, and its own connect view (`pane/place-connect.svelte.ts`,
 *   `pane/device-connect.svelte.ts` for a phone) dials, shows Cancel, words a refusal,
 *   and enters the folder. Nothing favorite-specific.
 * - **Every other reach**: the pane STAYS PUT (moving it onto an id with no row puts
 *   it on a volume the app says doesn't exist), and one info toast for that pane says
 *   why, with its one way out (`favorite-reach.ts` decides both).
 * - **No `favoriteTarget` at all** (a listing from before the reach pass): ask Rust
 *   which volume contains the path, and refuse as `not_found` when none does.
 *
 * The switch itself stays the caller's: the favorites menu hands its host a
 * `VolumeChangePayload`, while the palette / MCP route calls `navigate()` directly
 * and wants its `NavigateResult` back. `go` is that last mile, its return value
 * passes straight through, and the toast's "Show servers" rides it too.
 */

import { removeFavorite, resolvePathVolume, stripFavoritePrefix } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { addToast, addToastForPane } from '$lib/ui/toast'
import type { FavoriteReach } from '$lib/ipc/bindings'
import { NETWORK_VOLUME_PATH } from '../pane/navigate-refusals'
import type { VolumeChangePayload } from '../pane/types'
import type { VolumeInfo } from '../types'
import { refusalOf, wordFavoriteRefusal, type FavoriteRefusal, type FavoriteRefusalAction } from './favorite-reach'
import { reportFavoriteOpened, type FavoriteOpenedEvent } from './favorites-analytics'
import FavoriteRefusalToastContent from './FavoriteRefusalToastContent.svelte'

const log = getAppLogger('fileExplorer')

/** Where Settings keeps each phone backend's switch, for the "turned off" toast's button. */
const MTP_SETTINGS_SECTION = ['File systems', 'MTP (Android/Kindle/cameras)']
const ADB_SETTINGS_SECTION = ['File systems', 'Android (ADB)']

/**
 * Opened, carrying whatever `go` returned, or not, with the reach that refused it and
 * the sentence the toast showed (MCP `select_volume` replies with the same words).
 */
export type OpenFavoriteResult<T> =
  | { kind: 'opened'; opened: T }
  | { kind: 'not-opened'; refusal: FavoriteRefusal; message: string }

export interface OpenFavoriteArgs<T> {
  /** The favorite's row, straight off the volume list. */
  favorite: VolumeInfo
  /** The pane the pick is for: where a refusal's toast belongs. */
  pane: 'left' | 'right'
  /** Which surface made the pick, and how. The one emit of `favorite_opened` lives here. */
  picked: FavoriteOpenedEvent
  /** The last mile: put the pane on a volume at a path. */
  go: (target: VolumeChangePayload) => T
}

export async function openFavorite<T>(args: OpenFavoriteArgs<T>): Promise<OpenFavoriteResult<T>> {
  const { favorite, picked, go } = args
  const target = favorite.favoriteTarget
  if (!target) return openByPath(args)

  const refusal = refusalOf(target.reach)
  reportFavoriteOpened(picked, target.reach.kind)
  if (refusal || !target.volumeId || !target.volumeRoot) {
    // A `ready` / `connects` row always names its volume; one that doesn't can't be entered.
    return refuse(args, refusal ?? NOT_FOUND)
  }
  return {
    kind: 'opened',
    opened: go({ volumeId: target.volumeId, volumePath: target.volumeRoot, targetPath: favorite.path, exact: true }),
  }
}

const NOT_FOUND: FavoriteRefusal = { kind: 'not_found' }

/** The fallback for a row without a target: today's path lookup, refusing when nothing claims the path. */
async function openByPath<T>(args: OpenFavoriteArgs<T>): Promise<OpenFavoriteResult<T>> {
  const { favorite, picked, go } = args
  const { volume, timedOut } = await resolvePathVolume(favorite.path)
  const reach: FavoriteReach['kind'] = volume ? 'ready' : 'not_found'
  reportFavoriteOpened(picked, reach)
  if (!volume) {
    log.warn('Favorite points at a path no volume claims, so the pane stays put: {path} (timedOut: {timedOut})', {
      path: favorite.path,
      timedOut,
    })
    return refuse(args, NOT_FOUND)
  }
  return {
    kind: 'opened',
    opened: go({ volumeId: volume.id, volumePath: volume.path, targetPath: favorite.path, exact: true }),
  }
}

function refuse<T>(args: OpenFavoriteArgs<T>, refusal: FavoriteRefusal): OpenFavoriteResult<T> {
  const { favorite, pane, go } = args
  const { message, action } = wordFavoriteRefusal(favorite, refusal)
  log.info('Favorite {id} not opened, the pane stays put: {reach}', { id: favorite.id, reach: refusal.kind })
  addToastForPane(pane, FavoriteRefusalToastContent, {
    level: 'info',
    props: {
      message,
      actionLabel: action ? actionLabel(action) : null,
      onAction: () => {
        if (action) runAction(action, favorite, go)
      },
    },
  })
  return { kind: 'not-opened', refusal, message }
}

function actionLabel(action: FavoriteRefusalAction): string {
  switch (action) {
    case 'open-mtp-settings':
    case 'open-adb-settings':
      return tString('fileExplorer.navigation.favoriteUnreachable.openSettings')
    case 'open-servers':
      return tString('fileExplorer.navigation.favoriteUnreachable.openServers')
    case 'remove-favorite':
      return tString('fileExplorer.navigation.favoriteUnreachable.removeFavorite')
  }
}

function runAction(
  action: FavoriteRefusalAction,
  favorite: VolumeInfo,
  go: (target: VolumeChangePayload) => unknown,
): void {
  switch (action) {
    case 'open-mtp-settings':
      void openSettingsWindow('favorite-toast', MTP_SETTINGS_SECTION)
      return
    case 'open-adb-settings':
      void openSettingsWindow('favorite-toast', ADB_SETTINGS_SECTION)
      return
    case 'open-servers':
      go({ volumeId: 'network', volumePath: NETWORK_VOLUME_PATH, targetPath: NETWORK_VOLUME_PATH })
      return
    case 'remove-favorite':
      removeFavorite(stripFavoritePrefix(favorite.id)).catch(() => {
        addToast(tString('fileExplorer.navigation.removeFavoriteFailed'), { level: 'error' })
      })
      return
  }
}
