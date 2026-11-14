/**
 * Which `feature-status.json` feature shows up in which `/features` chapter, with its icon.
 * The chapters' prose and clips live in `src/pages/features.astro`; this module only places
 * the cards. Every JSON feature lands in exactly one chapter, and `placeFeatures` fails the
 * build when the JSON and this map drift, so a new feature can't silently miss the page.
 */
import type { IconName } from '../components/icons/icon-map'
import { features, type Feature } from './feature-status'

/** In page order. Each is also the chapter's anchor (`/features#connect`). */
export const CHAPTER_IDS = ['file-ops', 'browse', 'connect', 'rename-and-view', 'find', 'ai', 'planned'] as const

export type ChapterId = (typeof CHAPTER_IDS)[number]

/** Each chapter's short name: its eyebrow, its jump link, and its heading in `llms-full.txt`. */
export const chapterLabels: Record<ChapterId, string> = {
  'file-ops': 'File operations',
  browse: 'Browsing',
  connect: 'Connections',
  'rename-and-view': 'Rename and view',
  find: 'Search and indexing',
  ai: 'Optional AI',
  planned: 'Coming next',
}

interface CardSpec {
  id: string
  icon: IconName
  /** Marketing title override; defaults to the JSON name. */
  title?: string
}

const chapterCards: Record<ChapterId, CardSpec[]> = {
  'file-ops': [
    { id: 'file-operations', icon: 'copy' },
    { id: 'operation-log', icon: 'history' },
    { id: 'drag-and-drop', icon: 'pointer' },
  ],
  browse: [
    { id: 'file-explorer', icon: 'folder' },
    { id: 'tabs', icon: 'app-window' },
    { id: 'keyboard-shortcuts', icon: 'keyboard' },
    { id: 'command-palette', icon: 'square-chevron-right' },
    { id: 'go-to-path', icon: 'navigation' },
    { id: 'select-files', icon: 'list-checks' },
    { id: 'download-notifications', icon: 'bell' },
  ],
  connect: [
    { id: 'network-drives', icon: 'server' },
    { id: 'nfs', icon: 'folder' },
    { id: 's3', icon: 'container' },
    { id: 'sftp', icon: 'square-terminal' },
    { id: 'webdav', icon: 'globe' },
    { id: 'android-devices', icon: 'smartphone' },
    { id: 'android-adb', icon: 'usb' },
    { id: 'git-browser', icon: 'git-branch' },
    { id: 'restic-backups', icon: 'database' },
    { id: 'archives', icon: 'file-archive' },
  ],
  'rename-and-view': [
    { id: 'multi-rename', icon: 'replace-all' },
    { id: 'file-viewer', icon: 'eye' },
  ],
  find: [
    { id: 'full-disk-index', icon: 'database' },
    { id: 'search', icon: 'search' },
    { id: 'image-search', icon: 'scan-text' },
  ],
  ai: [
    { id: 'local-ai', icon: 'laptop-minimal-check', title: 'Local AI model' },
    { id: 'ask-cmdr', icon: 'messages-square' },
    { id: 'ai-rename', icon: 'brain' },
    { id: 'ai-batch', icon: 'zap' },
    { id: 'ai-tell-me', icon: 'message-circle-question-mark' },
  ],
  planned: [
    { id: 'disk-space-visualizer', icon: 'chart-pie' },
    { id: 'more-file-systems', icon: 'cloud' },
    { id: 'plugins', icon: 'puzzle' },
    { id: 'windows-linux', icon: 'monitor' },
  ],
}

export interface PlacedCard {
  icon: IconName
  title: string
  feature: Feature
}

/**
 * Resolves every chapter's cards against the JSON, and throws when a feature is missing,
 * unknown, or placed twice, or when a feature sits outside `planned` while being planned
 * (or the other way round), so the "what works today" chapters never promise a plan.
 */
function placeFeatures(): Record<ChapterId, PlacedCard[]> {
  const placed = CHAPTER_IDS.flatMap((chapter) => chapterCards[chapter].map((card) => ({ chapter, ...card })))
  const placedIds = placed.map((card) => card.id)
  const jsonIds = features.map((feature) => feature.id)
  const missing = jsonIds.filter((id) => !placedIds.includes(id))
  const unknown = placedIds.filter((id) => !jsonIds.includes(id))
  const dupes = placedIds.filter((id, i) => placedIds.indexOf(id) !== i)
  const idToFeatureMap = new Map(features.map((feature) => [feature.id, feature]))
  const misfiled = placed
    .filter((card) => {
      const feature = idToFeatureMap.get(card.id)
      return feature && (feature.status === 'planned') !== (card.chapter === 'planned')
    })
    .map((card) => card.id)
  if (missing.length || unknown.length || dupes.length || misfiled.length) {
    throw new Error(
      `features page out of sync with feature-status.json. Missing: [${missing.join(', ')}], unknown: [${unknown.join(', ')}], duplicated: [${dupes.join(', ')}], planned status and chapter disagree: [${misfiled.join(', ')}]`,
    )
  }
  return Object.fromEntries(
    CHAPTER_IDS.map((chapter) => [
      chapter,
      chapterCards[chapter].map((card) => {
        const feature = idToFeatureMap.get(card.id)!
        return { icon: card.icon, title: card.title ?? feature.name, feature }
      }),
    ]),
  ) as Record<ChapterId, PlacedCard[]>
}

export const cardsByChapter = placeFeatures()
