/**
 * What a pane does when the user opens an entry: Enter, ⌘↓, a double-click, or
 * a choice from the Enter-behavior popup. One decision chain, in order:
 *
 * 1. `redirectToPath` (a backend-marked virtual entry: git worktree / submodule
 *    working dirs) browses to the target instead of the entry's own path;
 * 2. the archive / bundle Enter policy (`browse` / `open` / `ask`), skipped once
 *    the PANE itself is inside an archive;
 * 3. a directory or archive file browses in place;
 * 4. a file inside an archive goes to the viewer (its inner path doesn't exist
 *    on disk, so the OS open would be a silent no-op);
 * 5. anything else goes to its default app.
 *
 * On a search-results pane every arm that would browse or launch instead LEAVES
 * the snapshot volume first: `resolveLocationOrToast` resolves the row's real
 * volume, and the parent's `navigate()` switches to it. Skipping that leaves the
 * pane on `search-results://` with a real path, which renders as "Search results
 * no longer available".
 */

import { openFile } from '$lib/tauri-commands'
import type { FileEntry } from '../types'
import type { Location } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { basenameOf, type CanonicalPath } from '$lib/path/canonical'
import { pathInsideArchive } from './volume-capabilities'
import { resolveEnterPolicy, parseEnterBehaviorOverrides } from './archive-enter-policy'
import { openFileViewer } from '$lib/file-viewer/open-viewer'
import { resolveLocationOrToast } from '../navigation/navigate-and-select'
import type { LoadDirectoryArgs } from './types'

export interface EntryActivationDeps {
  getCurrentPath: () => string
  setCurrentPath: (path: string) => void
  /** `currentPath` with `~` expanded, for the folder-we-came-from lookup. */
  getCanonicalPath: () => CanonicalPath | null
  /** The pane's DRIVE volume id (an archive pane keeps its parent drive's id). */
  getVolumeId: () => string
  getIsSearchResultsView: () => boolean
  loadDirectory: (args: LoadDirectoryArgs) => Promise<void> | void
  /** Show the Browse | Open | Configure popup for an entry set to Ask. */
  openEnterMenu: (entry: FileEntry) => void
  /** Bubble a resolved location so the parent's `navigate()` switches volumes. */
  onGoToLocation: (location: Location) => void
}

export interface EntryActivation {
  /** Open an entry: the full decision chain above. */
  handleNavigate: (entry: FileEntry) => Promise<void>
  /** Step into a folder / archive / bundle. Also the popup's Browse choice. */
  browseIntoEntry: (entry: FileEntry) => Promise<void>
  /** Hand an entry to its default app. Also the popup's Open choice. */
  openEntryExternally: (entry: FileEntry) => Promise<void>
}

export function createEntryActivation(deps: EntryActivationDeps): EntryActivation {
  /**
   * Step into a folder / archive / bundle: commit the path and load its listing.
   * When going up (`..`), remember the folder we came from so it lands selected.
   */
  async function browseIntoEntry(entry: FileEntry): Promise<void> {
    const isGoingUp = entry.name === '..'
    const canonical = deps.getCanonicalPath()
    const currentFolderName = isGoingUp && canonical ? basenameOf(canonical) : undefined
    deps.setCurrentPath(entry.path)
    // Note: onPathChange is called in the listing-complete handler after a successful load.
    await deps.loadDirectory({ path: entry.path, selectName: currentFolderName })
  }

  /**
   * Hand an entry to its default app via LaunchServices: a `.zip` opens in the OS
   * archive tool, a `.app`/`.bundle` launches, any other file opens normally.
   */
  async function openEntryExternally(entry: FileEntry): Promise<void> {
    try {
      await openFile(entry.path)
    } catch {
      // Silently fail - file open errors are expected sometimes
    }
  }

  /**
   * Leave the search-results pane for a real entry: resolve `realPath` to a
   * `Location` (volume id + path), then bubble it via `onGoToLocation` so
   * `navigate()` switches to the real volume before loading the path. An
   * unresolvable path (its drive is gone) shows the shared friendly toast rather
   * than navigating to the wrong volume.
   */
  async function goToRealEntry(realPath: string): Promise<void> {
    const location = await resolveLocationOrToast(realPath)
    if (!location) return
    deps.onGoToLocation(location)
  }

  async function handleNavigate(entry: FileEntry): Promise<void> {
    // `redirectToPath` is set by the backend on virtual entries that
    // should open elsewhere (worktree and submodule working dirs).
    if (entry.redirectToPath) {
      if (deps.getIsSearchResultsView()) {
        await goToRealEntry(entry.redirectToPath)
        return
      }
      deps.setCurrentPath(entry.redirectToPath)
      await deps.loadDirectory({ path: entry.redirectToPath })
      return
    }
    // Enter-behavior policy for archives (`.zip`) and macOS bundles (`.app`
    // etc.). Gate on the PANE's path, not the entry's: `pathInsideArchive` is true
    // for a `.zip` file ITSELF (its own path crosses the boundary), so gating on
    // the entry would wrongly skip the archive we want the popup for. Gating on the
    // current directory skips the policy only when we're already browsing INSIDE an
    // archive — there the entries are inner items, which keep the viewer interim
    // below. `browse` falls through to the folder-browse arm; `open` launches;
    // `ask` shows the Browse | Open | Configure popup.
    if (!pathInsideArchive(deps.getCurrentPath())) {
      const action = resolveEnterPolicy(entry, parseEnterBehaviorOverrides(getSetting('behavior.archiveEnterBehavior')))
      if (action) {
        // From search results, opening any real entry must switch to its real
        // volume first (no popup on the snapshot pane — mirrors the arms below).
        if (deps.getIsSearchResultsView()) {
          await goToRealEntry(entry.path)
          return
        }
        if (action === 'ask') {
          deps.openEnterMenu(entry)
          return
        }
        if (action === 'open') {
          await openEntryExternally(entry)
          return
        }
        // action === 'browse': fall through to the folder-browse arm.
      }
    }
    // An archive file (`.zip`) browses like a folder: Enter navigates INTO it,
    // same-volume in-place (the tab keeps the parent-drive id; the backend
    // `resolve` routes the `…/foo.zip/…` path to the read-only ArchiveVolume).
    // `isDirectory` stays false on an archive, so it's an explicit second arm.
    if (entry.isDirectory || entry.isArchive) {
      // Same as the redirect branch: a real directory opened from the
      // search-results rows switches to its real volume first.
      if (deps.getIsSearchResultsView()) {
        await goToRealEntry(entry.path)
        return
      }
      await browseIntoEntry(entry)
    } else if (pathInsideArchive(entry.path)) {
      // A file INSIDE an archive can't be opened by the OS default app: the
      // inner path doesn't exist on disk, so `openFile` is a silent no-op.
      // Route to the viewer (bounded temp-extract, same as F3) — the honest
      // interim until the Enter-behavior milestone adds extract-then-open.
      // Pass the pane's DRIVE volume id (an archive pane keeps its parent
      // drive's id) so a remote-hosted zip previews through that volume.
      void openFileViewer(entry.path, deps.getVolumeId())
    } else {
      await openEntryExternally(entry)
    }
  }

  return { handleNavigate, browseIntoEntry, openEntryExternally }
}
