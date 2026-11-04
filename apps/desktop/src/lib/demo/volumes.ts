/**
 * The demo's volume list: one main disk plus the usual favorites, all on the in-memory volume.
 */
import type { LocationCategory, LocationInfo } from '#lib/ipc/bindings.js'
import type { AnswerTable } from './answer-types.js'
import { DEMO_HOME, DEMO_VOLUME_ID, demoEntries, type DemoEntry } from './demo-tree.js'

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
    supportsTrash: false,
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

const kindByPath = new Map<string, DemoEntry['kind']>([
  ['/', 'dir'],
  ...demoEntries.map((e): [string, DemoEntry['kind']] => [e.path, e.kind]),
])

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
  pathExists: (p) => ({ data: kindByPath.has(demoPath(p.path as string)), timedOut: false }),
  // `true` for a folder, `false` for a file, `null` for a path the tree doesn't hold. A symlink
  // counts as unknown: the drop path that asks degrades to its approximate count.
  statPathsKinds: (p) => ({
    data: (p.paths as string[]).map((raw) => {
      const kind = kindByPath.get(demoPath(raw))
      return kind === 'dir' ? true : kind === 'file' ? false : null
    }),
    timedOut: false,
  }),
  // Nothing on the demo volume syncs, and it stores every path as given.
  getSyncStatus: () => ({ data: {}, timedOut: false }),
  storedSpellings: (p) => ({ data: p.paths as string[], timedOut: false }),
}
