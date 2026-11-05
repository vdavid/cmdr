/**
 * What a favorite row's reach (`VolumeInfo.favoriteTarget.reach`, decided by Rust's
 * `favorites/reach.rs`) means to a person: the status line under its path in the
 * favorites menu, and the toast a pick raises when the pane can't go there.
 *
 * Pure words and one decision table, so the menu, the toast, and MCP `select_volume`
 * read every reach the same way. ❗ Each switch is exhaustive over the binding's union,
 * so a new reach kind is a type error here rather than a silent "ready".
 */

import type { FavoriteReach, FavoriteTarget } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import type { IconName } from '#lib/ui/icons/icon-map.js'
import { volumeScheme, type VolumeScheme } from '#lib/volume-scheme.js'
import type { VolumeInfo } from '../types'

/** A reach the pane can't follow: a pick leaves the pane where it is and says why. */
export type FavoriteRefusal = Exclude<FavoriteReach, { kind: 'ready' } | { kind: 'connects' }>

/** The one way out a refusal toast offers, when there is one. */
export type FavoriteRefusalAction = 'open-mtp-settings' | 'open-adb-settings' | 'open-servers' | 'remove-favorite'

/** `null` when a pick goes through (`ready`, or `connects`: the pane's own connect view dials). */
export function refusalOf(reach: FavoriteReach): FavoriteRefusal | null {
  switch (reach.kind) {
    case 'ready':
    case 'connects':
      return null
    case 'unplugged':
    case 'access_off':
    case 'forgotten':
    case 'not_found':
      return reach
  }
}

/** The volume's name as the words say it: the row's, else the one stored with the favorite, else the favorite's own label. */
function placeName(favorite: VolumeInfo): string {
  return favorite.favoriteTarget?.volumeName ?? favorite.name
}

/**
 * What kind of place the volume is, for the words' `{kind}` select: case languages put a
 * type noun before the name ("к серверу «…»"), so the sentence has to know which one.
 */
type PlaceKind = 'share' | 'server' | 'phone' | 'other'

function placeKind(favorite: VolumeInfo): PlaceKind {
  const volumeId = favorite.favoriteTarget?.volumeId
  return volumeId ? PLACE_KINDS[volumeScheme(volumeId)] : 'other'
}

/** Each scheme's place kind. A `Record`, so a new scheme can't compile without one. */
const PLACE_KINDS: Record<VolumeScheme, PlaceKind> = {
  smb: 'share',
  nfs: 'share',
  sftp: 'server',
  webdav: 'server',
  s3: 'server',
  mtp: 'phone',
  adb: 'phone',
  root: 'other',
  local: 'other',
  path: 'other',
  cloud: 'other',
  favorite: 'other',
  unknown: 'other',
}

/** What a refused pick's toast says, and the one action it offers. */
export function wordFavoriteRefusal(
  favorite: VolumeInfo,
  refusal: FavoriteRefusal,
): { message: string; action: FavoriteRefusalAction | null } {
  switch (refusal.kind) {
    case 'unplugged':
      return { message: wordUnplugged(placeName(favorite), refusal), action: null }
    case 'access_off':
      return refusal.backend === 'mtp'
        ? { message: tString('fileExplorer.navigation.favoriteUnreachable.mtpOff'), action: 'open-mtp-settings' }
        : { message: tString('fileExplorer.navigation.favoriteUnreachable.adbOff'), action: 'open-adb-settings' }
    case 'forgotten':
      return {
        message: tString('fileExplorer.navigation.favoriteUnreachable.forgotten', {
          place: placeName(favorite),
          kind: placeKind(favorite),
        }),
        action: 'open-servers',
      }
    case 'not_found':
      return {
        message: tString('fileExplorer.navigation.favoriteUnreachable.notFound', {
          name: favorite.name,
          path: favorite.path,
        }),
        action: 'remove-favorite',
      }
  }
}

function wordUnplugged(place: string, refusal: Extract<FavoriteRefusal, { kind: 'unplugged' }>): string {
  switch (refusal.device) {
    case 'phone':
      // A listed phone that can't be used has a more precise story than "plug it in".
      if (refusal.reason === 'offline') return tString('adb.readiness.offline')
      if (refusal.reason === 'no_permissions') return tString('adb.readiness.noPermissions')
      return tString('fileExplorer.navigation.favoriteUnreachable.phoneUnplugged', { device: place })
    case 'storage':
      return tString('fileExplorer.navigation.favoriteUnreachable.storageUnplugged', { device: place })
    case 'drive':
      return tString('fileExplorer.navigation.favoriteUnreachable.driveUnplugged', { drive: place })
  }
}

/** The line under a favorite's path in its menu tooltip, or `null` when a pick opens it right away. */
export function favoriteReachStatus(favorite: VolumeInfo): string | null {
  const target = favorite.favoriteTarget
  if (!target) return null
  const place = placeName(favorite)
  const reach = target.reach
  switch (reach.kind) {
    case 'ready':
      return null
    case 'connects':
      return tString('fileExplorer.navigation.favoriteStatus.connects', { place, kind: placeKind(favorite) })
    case 'unplugged':
    case 'access_off':
    case 'forgotten':
      // The toast's first sentence is already the status: the same words in both places.
      return wordFavoriteRefusal(favorite, reach).message
    case 'not_found':
      return tString('fileExplorer.navigation.favoriteStatus.notFound')
  }
}

/** Whether the menu renders the row dimmed: anything a pick can't open right away. */
export function favoriteIsDimmed(favorite: VolumeInfo): boolean {
  return favoriteReachStatus(favorite) !== null
}

/**
 * The glyph a favorite shows when it has no icon of its own (no NSWorkspace icon for a
 * folder on a share, server, or phone): by the kind of volume it lives on.
 */
export function favoriteFallbackGlyph(target: FavoriteTarget | null | undefined): IconName {
  if (!target?.volumeId) return 'folder'
  return FALLBACK_GLYPHS[volumeScheme(target.volumeId)]
}

/** Each scheme's glyph. A `Record`, so a new scheme can't compile without one. */
const FALLBACK_GLYPHS: Record<VolumeScheme, IconName> = {
  smb: 'server',
  sftp: 'server',
  webdav: 'server',
  s3: 'server',
  nfs: 'server',
  mtp: 'smartphone',
  adb: 'smartphone',
  root: 'folder',
  local: 'folder',
  path: 'folder',
  cloud: 'folder',
  favorite: 'folder',
  unknown: 'folder',
}
