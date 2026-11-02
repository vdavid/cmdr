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
 * 5. anything else goes to its default app, with a short "Opening…" toast
 *    (`opening-toast.ts`).
 *
 * On a search-results pane every arm that would browse or launch instead LEAVES
 * the snapshot volume first: `resolveLocationOrToast` resolves the row's real
 * volume, and the parent's `navigate()` switches to it. Skipping that leaves the
 * pane on `search-results://` with a real path, which renders as "Search results
 * no longer available".
 */

import { openFile } from '#lib/tauri-commands/index.js'
import type { FileEntry } from '../types'
import type { Location } from '#lib/tauri-commands/index.js'
import { getSetting } from '#lib/settings/index.js'
import { basenameOf, type CanonicalPath } from '#lib/path/canonical.js'
import { pathCrossesArchiveBoundary, pathInsideArchive } from './archive-paths'
import { resolveEnterPolicy, enterBehaviorFromSettings, type EnterBehaviorByFormat } from './archive-enter-policy'
import { openFileViewer } from '#lib/file-viewer/open-viewer.js'
import { resolveLocationOrToast } from '../navigation/navigate-and-select'
import type { LoadDirectoryArgs } from './types'
import { announceOpening } from './opening-toast'

/**
 * What opening an entry does: the decision chain above, with no side effects. `launch` and `choose`
 * can run something outside Cmdr, so they're what `open_under_cursor` asks a person about.
 */
export type ActivationKind = 'browse' | 'viewer' | 'launch' | 'choose'

export function resolveActivation(
  entry: FileEntry,
  context: { panePath: string; isSearchResultsView: boolean; behavior: EnterBehaviorByFormat },
): ActivationKind {
  if (entry.redirectToPath) return 'browse'
  // Gate on the PANE's path, not the entry's, and with the WIDE check: a pane sitting AT
  // `/a/foo.zip` is already showing the archive's contents, so the policy has had its say and the
  // entries below are inner items. Gating on the entry instead would skip the very archive we want
  // the popup for.
  if (!pathCrossesArchiveBoundary(context.panePath)) {
    const action = resolveEnterPolicy(entry, context.behavior)
    if (action) {
      // From search results, opening any real entry must switch to its real volume first (no popup
      // on the snapshot pane).
      if (context.isSearchResultsView) return 'browse'
      if (action === 'ask') return 'choose'
      if (action === 'open') return 'launch'
      // `browse` falls through: a document package set to Browse is still a plain file here.
    }
  }
  // An archive file (`.zip`) browses like a folder, same-volume in place. `isDirectory` stays false
  // on an archive, so it's an explicit second arm.
  if (entry.isDirectory || entry.isArchive) return 'browse'
  // The NARROW check: only a path with a real inner part is unopenable. The inner path doesn't exist
  // on disk, so the OS open would be a silent no-op: the viewer (bounded temp-extract, same as F3) is
  // the honest interim until extract-then-launch lands (`crates/cmdr-archive/DETAILS.md` § "Left for
  // later").
  if (pathInsideArchive(entry.path)) return 'viewer'
  return 'launch'
}

/** What `open_under_cursor`'s plan reads (`tools/ask_plans.rs`). */
export interface CursorItemDescription {
  activation: ActivationKind
  path: string
  name: string
  isDirectory: boolean
}

/**
 * What a tool's open must still be true of when it happens: the path a person agreed to, or, when
 * nobody was asked, that it launches nothing. Checked here because this is the last moment the
 * cursor can't move.
 */
export type ToolOpenGate = { expectedPath: string } | { mayLaunch: false }

export type ToolOpenAnswer = 'opened' | 'changedWhileAsking'

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
  /** What opening an entry would do, for a tool's question. */
  describe: (entry: FileEntry) => CursorItemDescription
  /** Open an entry for a tool, unless the gate says it isn't what was agreed to. */
  openForTool: (entry: FileEntry, gate: ToolOpenGate) => Promise<ToolOpenAnswer>
}

export function createEntryActivation(deps: EntryActivationDeps): EntryActivation {
  /**
   * Step into a folder / archive / bundle: commit the path and load its listing.
   * When going up (`..`), remember the folder we came from so it lands selected.
   */
  async function browseIntoEntry(entry: FileEntry): Promise<void> {
    const isGoingUp = entry.name === '..'
    const canonical = deps.getCanonicalPath()
    // The `..` row carries the PARENT of the current path, which `FilePane` can
    // only work out once the pane's path canonicalizes. Until it does the row
    // falls back to the empty path, and the empty path is how the backend spells
    // "this volume's root" — so navigating it would silently leap the whole way
    // up instead of one level. Going nowhere is the honest answer, and it matches
    // what Backspace does in the same state (`listing-loader.ts`).
    if (isGoingUp && !entry.path) return
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
    // Side by side with the launch, never in front of it: the toast is the
    // feedback a slow share otherwise lacks.
    void announceOpening(entry)
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

  function activationOf(entry: FileEntry): ActivationKind {
    return resolveActivation(entry, {
      panePath: deps.getCurrentPath(),
      isSearchResultsView: deps.getIsSearchResultsView(),
      behavior: enterBehaviorFromSettings(getSetting),
    })
  }

  async function handleNavigate(entry: FileEntry): Promise<void> {
    switch (activationOf(entry)) {
      case 'choose':
        deps.openEnterMenu(entry)
        return
      case 'launch':
        await openEntryExternally(entry)
        return
      case 'viewer':
        // Pass the pane's DRIVE volume id (an archive pane keeps its parent
        // drive's id) so a remote-hosted zip previews through that volume.
        void openFileViewer(entry.path, deps.getVolumeId())
        return
      case 'browse':
        if (deps.getIsSearchResultsView()) {
          await goToRealEntry(entry.redirectToPath ?? entry.path)
        } else if (entry.redirectToPath) {
          deps.setCurrentPath(entry.redirectToPath)
          await deps.loadDirectory({ path: entry.redirectToPath })
        } else {
          await browseIntoEntry(entry)
        }
    }
  }

  function describe(entry: FileEntry): CursorItemDescription {
    return { activation: activationOf(entry), path: entry.path, name: entry.name, isDirectory: entry.isDirectory }
  }

  /** `handleNavigate` for a tool, checked against the gate at the moment it opens. */
  async function openForTool(entry: FileEntry, gate: ToolOpenGate): Promise<ToolOpenAnswer> {
    const activation = activationOf(entry)
    const launches = activation === 'launch' || activation === 'choose'
    if ('expectedPath' in gate) {
      if (entry.path !== gate.expectedPath) return 'changedWhileAsking'
      if (launches) {
        // The person already said to open it. The Browse | Open popup would ask again, a stray
        // Enter away from its first row.
        await openEntryExternally(entry)
        return 'opened'
      }
    } else if (launches) {
      return 'changedWhileAsking'
    }
    await handleNavigate(entry)
    return 'opened'
  }

  return { handleNavigate, browseIntoEntry, openEntryExternally, describe, openForTool }
}
