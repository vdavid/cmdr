/**
 * The two scope presets the Search dialog offers, and the default it falls back on.
 *
 * A search covers at most one volume (`src-tauri/src/search/execute.rs`), so the scope
 * ladder is exactly two rungs: the focused pane's **current folder** (the default) and
 * **this volume** (the maximum). Both are resolved here, purely, from the focused pane's
 * path and history plus the live volume roots, so the dialog plumbing stays testable.
 *
 * The current folder can be missing: the focused pane may be a `search-results://<id>`
 * snapshot, whose path isn't a real folder the index can search inside. Then we walk the
 * pane's navigation history backward for the most recent real folder, and if there's none
 * we fall back to the volume — a search still runs, one rung wider.
 */
import { tString } from '$lib/intl/messages.svelte'
import type { ScopePresets } from '$lib/query-ui/query-dialog-config'
import type { LocationCategory, VolumeInfo } from '$lib/file-explorer/types'

const SEARCH_RESULTS_PREFIX = 'search-results://'

/** The boot volume: every path is under it, so it's the last-resort scope. */
const BOOT_VOLUME_ROOT = '/'

export interface SearchScopeInput {
  /** Current path of the focused pane (may be `search-results://<id>`). */
  currentPath: string
  /**
   * Stack of recent paths from the focused pane's navigation history, ordered oldest first
   * (matches `NavigationHistory.stack`). The current path is typically the last entry, but
   * we don't depend on that — we just skip every `search-results://` entry when scanning
   * backward.
   */
  history: string[]
  /** Mount roots of the live volumes (`VolumeInfo.path`), in any order. */
  volumeRoots: string[]
}

/** Which preset an unset scope resolves to, and the path it hands the backend. */
export interface DefaultScope {
  path: string
  kind: 'currentFolder' | 'thisVolume'
}

/**
 * Picks the scope presets for the focused pane. Three cases for the current folder:
 *   1. The pane is on a real folder: use it as-is.
 *   2. The pane is on `search-results://...` AND its history has a real-folder entry: use
 *      the most recent such entry.
 *   3. The pane is on `search-results://...` with no real-folder entry: `null` plus the
 *      canonical tooltip. The default scope then falls back to the volume.
 */
export function resolveSearchScope({ currentPath, history, volumeRoots }: SearchScopeInput): ScopePresets {
  const currentFolder = resolveCurrentFolder(currentPath, history)
  return {
    currentFolder,
    currentFolderUnavailableReason: currentFolder === null ? tString('search.searchableFolder.disabledTooltip') : '',
    volumeRoot: volumeRootFor(volumeRoots, currentFolder),
  }
}

/** The pane's current folder, or `null` when only snapshot paths are reachable. */
function resolveCurrentFolder(currentPath: string, history: string[]): string | null {
  if (!currentPath.startsWith(SEARCH_RESULTS_PREFIX)) return currentPath
  // Walk backward through history for the newest non-snapshot path.
  for (let i = history.length - 1; i >= 0; i--) {
    const entry = history[i]
    if (!entry.startsWith(SEARCH_RESULTS_PREFIX)) return entry
  }
  return null
}

/**
 * The categories in the switcher's volume list that OWN an index, and so can be the
 * target of a search. The two that are left out are left out on purpose:
 *
 * - **`favorite`** entries are plain folders wearing a volume's clothes (`Downloads`,
 *   `Documents`). Counting one as a volume root makes "this volume" resolve to
 *   `~/Downloads` while searching inside it, so the maximum rung collapses onto the
 *   default one and a user asking for the whole drive quietly gets one folder.
 * - **`cloud_drive`** entries (Dropbox, iCloud Drive, Google Drive) live inside the boot
 *   volume's index and route to `root` in the backend
 *   (`crates/cmdr-index/.../paths/routing.rs`, and the plan's Decision 16). Treating one
 *   as its own volume would disagree with the routing that answers the search.
 */
const INDEX_OWNING_CATEGORIES: readonly LocationCategory[] = [
  'main_volume',
  'attached_volume',
  'network',
  'mobile_device',
]

/** The mount roots a search can target, out of the live volume list. */
export function volumeRootsFrom(volumes: VolumeInfo[]): string[] {
  return volumes.filter((v) => v.path && INDEX_OWNING_CATEGORIES.includes(v.category)).map((v) => v.path)
}

/**
 * The mount root `path` sits on: the longest volume root that contains it. Falls back to
 * the boot volume when nothing matches (no path at all, or a volume that has since
 * unmounted), which is also the right answer for an ordinary boot-disk path.
 */
export function volumeRootFor(volumeRoots: string[], path: string | null): string {
  if (path === null) return BOOT_VOLUME_ROOT
  let best = BOOT_VOLUME_ROOT
  for (const root of volumeRoots) {
    if (!isUnder(path, root)) continue
    if (root.length > best.length) best = root
  }
  return best
}

/** Whether `path` is `root` itself or lives inside it, matching whole path segments. */
function isUnder(path: string, root: string): boolean {
  if (path === root) return true
  const prefix = root.endsWith('/') ? root : `${root}/`
  return path.startsWith(prefix)
}

/**
 * What an unset scope means. The current folder when there is one, else the volume — so a
 * search always has a target, and the empty scope box never means "everywhere".
 */
export function resolveDefaultScope(presets: ScopePresets): DefaultScope {
  return presets.currentFolder === null
    ? { path: presets.volumeRoot, kind: 'thisVolume' }
    : { path: presets.currentFolder, kind: 'currentFolder' }
}

/** The default scope's name, for the Search-in chip and the scope box's placeholder. */
export function defaultScopeLabel(kind: DefaultScope['kind']): string {
  return kind === 'thisVolume' ? tString('queryUi.scope.thisVolume') : tString('queryUi.scope.currentFolder')
}
