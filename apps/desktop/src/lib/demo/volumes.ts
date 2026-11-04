/**
 * The demo's volume list: one main disk plus the usual favorites, all on the in-memory volume.
 */
import type { LocationCategory, LocationInfo } from '#lib/ipc/bindings.js'
import type { AnswerTable } from './answer-types.js'
import { DEMO_HOME, DEMO_VOLUME_ID } from './demo-tree.js'

function location(
  id: string,
  name: string,
  path: string,
  category: LocationCategory,
  favoriteShortcut: string | null = null,
): LocationInfo {
  return {
    id,
    name,
    path,
    category,
    icon: null,
    isEjectable: false,
    fsType: 'apfs',
    // The in-memory volume has no Trash (`Volume::has_trash`), so F8 deletes for good after the
    // app's own confirm, as on a network share.
    supportsTrash: false,
    // Writes land, in memory, through the real write engine (`crates/cmdr-listing-wasm`).
    mountIsReadOnly: false,
    isDiskImage: false,
    sharesDiskWithOthers: false,
    isCloudMount: false,
    connectionState: null,
    pinned: null,
    landingPath: null,
    deviceReadiness: null,
    usbSpeed: null,
    capabilities: null,
    favoriteShortcut,
    favoriteTarget:
      category === 'favorite'
        ? { volumeId: DEMO_VOLUME_ID, volumeName: 'Macintosh HD', volumeRoot: '/', reach: { kind: 'ready' } }
        : null,
    rootLabel: null,
    mountAccount: null,
  }
}

const mainVolume = location(DEMO_VOLUME_ID, 'Macintosh HD', '/', 'main_volume')

const demoLocations: LocationInfo[] = [
  location('fav-home', 'you', DEMO_HOME, 'favorite', 'h'),
  location('fav-desktop', 'Desktop', `${DEMO_HOME}/Desktop`, 'favorite', 'd'),
  location('fav-documents', 'Documents', `${DEMO_HOME}/Documents`, 'favorite', 'o'),
  location('fav-downloads', 'Downloads', `${DEMO_HOME}/Downloads`, 'favorite', 'w'),
  mainVolume,
]

/** A path as the demo volume spells it: `~` expanded, no trailing slash. */
export function demoPath(raw: string): string {
  const expanded = raw === '~' || raw.startsWith('~/') ? DEMO_HOME + raw.slice(1) : raw
  return expanded.length > 1 && expanded.endsWith('/') ? expanded.slice(0, -1) : expanded
}

export const volumeAnswers: AnswerTable = {
  listVolumes: () => ({ data: demoLocations, timedOut: false }),
  getDefaultVolumeId: () => DEMO_VOLUME_ID,
  getE2eStartPath: () => null,
  resolvePathVolume: () => ({ volume: mainVolume, timedOut: false }),
  // `pathExists` and `statPathsKinds` are the engine's: the volume they ask changes with every write.
  // Nothing on the demo volume syncs, and it stores every path as given.
  getSyncStatus: () => ({ data: {}, timedOut: false }),
  storedSpellings: (p) => ({ data: p.paths as string[], timedOut: false }),
}
