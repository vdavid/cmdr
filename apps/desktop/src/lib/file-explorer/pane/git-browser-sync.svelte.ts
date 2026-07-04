/**
 * Git browser sync for a file pane: keeps the breadcrumb repo chip and the
 * file-list git-status column fed with live `RepoInfo` for the pane's current
 * path. Owns the two feature-toggle mirrors (kept in sync with Settings via
 * change subscriptions), the lazy repo lookup / subscribe lifecycle, and the
 * reactive `$effect` that re-runs it whenever the path or either toggle changes.
 *
 * Lifted out of `FilePane.svelte` into a `*.svelte.ts` factory owning the
 * `$effect` (created synchronously during component init, the
 * `initListingDiffSync` pattern) plus its subscription cleanup.
 *
 * Lookups are best-effort: a non-git path (or a network / MTP volume that can't
 * host a repo) leaves `gitRepoInfo` null and drops any active subscription.
 */

import { lookupRepoInfo, subscribeToRepo, unsubscribeFromRepo, type RepoInfo } from '../git/git-store.svelte'
import { getSetting, onSpecificSettingChange } from '$lib/settings'
import { isMtpVolumeId } from '$lib/mtp'
import { pathInsideArchive } from './volume-capabilities'

export interface GitBrowserSyncDeps {
  /** The pane's current directory path (reactive read). */
  getCurrentPath: () => string
  /** The pane's volume id, for the MTP-transport git skip (reactive read). */
  getVolumeId: () => string
  /**
   * Whether the pane's volume kind has a real backend listing that could host a
   * git repo (reactive read off the pane's derived caps). Network / search-
   * results panes are false; MTP is true (but skipped separately — git can't run
   * over the MTP transport).
   */
  getHasBackendListing: () => boolean
}

export interface GitBrowserSync {
  /** Live `RepoInfo` for the current path, or null when disabled / non-git. */
  readonly gitRepoInfo: RepoInfo | null
  /** Whether the breadcrumb repo chip is enabled (Settings mirror). */
  readonly showRepoChip: boolean
  /** Whether the file-list git-status column is enabled (Settings mirror). */
  readonly showGitStatusColumn: boolean
  /**
   * Drops the two setting subscriptions and any active repo subscription. Call
   * from the owning component's `onDestroy`. Without the setting-subscription
   * drop the listeners would outlive the pane (panes re-mount on tab switch /
   * swap), mutating a dead component's `$state`.
   */
  cleanup: () => void
}

export function createGitBrowserSync(deps: GitBrowserSyncDeps): GitBrowserSync {
  let gitRepoInfo = $state<RepoInfo | null>(null)
  // Plain bookkeeping (not $state): only read inside `syncGitState` and cleanup,
  // never rendered. The repo root of the subscription `gitRepoInfo` came from.
  let activeRepoRoot: string | null = null
  let showRepoChip = $state<boolean>(getSetting('fileExplorer.git.showRepoChip'))
  let showGitStatusColumn = $state<boolean>(getSetting('fileExplorer.git.showStatusColumn'))

  const unsubscribeChipSetting = onSpecificSettingChange('fileExplorer.git.showRepoChip', (_id, v) => {
    showRepoChip = v
  })
  const unsubscribeColumnSetting = onSpecificSettingChange('fileExplorer.git.showStatusColumn', (_id, v) => {
    showGitStatusColumn = v
  })

  /**
   * Drives the chip's and status column's data: looks up the repo for `path`,
   * subscribes to live updates if it's a new repo, and unsubscribes when the
   * path leaves the previous repo. Runs whenever either feature is enabled (both
   * read from `gitRepoInfo`). When both are off (or on a network / MTP volume
   * that can't host a git repo), the subscription is dropped.
   */
  async function syncGitState(path: string): Promise<void> {
    const gitFeaturesNeeded = showRepoChip || showGitStatusColumn
    // The virtual-volume half (network / search-results) folds into
    // `!getHasBackendListing()` (no real directory to host a repo). The
    // `isMtpVolumeId` check STAYS: MTP DOES have a backend listing but git can't
    // run over the MTP transport, so it's an MTP-path-specific skip, not a
    // capability question. Archives ALSO have a backend listing (so
    // `getHasBackendListing()` is true), but a git repo can't live inside a zip —
    // an explicit `pathInsideArchive` skip, since `hasBackendListing` doesn't cover
    // it (a `lookupRepoInfo` on a `…/foo.zip/…` path would walk out of the archive).
    if (
      !gitFeaturesNeeded ||
      isMtpVolumeId(deps.getVolumeId()) ||
      !deps.getHasBackendListing() ||
      pathInsideArchive(path)
    ) {
      if (activeRepoRoot) {
        await unsubscribeFromRepo(activeRepoRoot)
        activeRepoRoot = null
        gitRepoInfo = null
      }
      return
    }
    const info = await lookupRepoInfo(path).catch(() => null)
    if (!info) {
      if (activeRepoRoot) {
        await unsubscribeFromRepo(activeRepoRoot)
        activeRepoRoot = null
        gitRepoInfo = null
      }
      return
    }
    if (activeRepoRoot && activeRepoRoot !== info.repoRoot) {
      await unsubscribeFromRepo(activeRepoRoot)
      activeRepoRoot = null
    }
    if (!activeRepoRoot) {
      try {
        gitRepoInfo = await subscribeToRepo(info.repoRoot)
        activeRepoRoot = info.repoRoot
      } catch {
        gitRepoInfo = info
      }
    } else {
      gitRepoInfo = info
    }
  }

  // Sync the git chip and status column whenever the path changes (or either
  // toggle flips). Kept tiny and side-effecting; the actual repo lookup lives in
  // `syncGitState` so it can be exercised without a reactive context.
  $effect(() => {
    const path = deps.getCurrentPath()
    void showRepoChip
    void showGitStatusColumn
    void syncGitState(path)
  })

  return {
    get gitRepoInfo() {
      return gitRepoInfo
    },
    get showRepoChip() {
      return showRepoChip
    },
    get showGitStatusColumn() {
      return showGitStatusColumn
    },
    cleanup: () => {
      unsubscribeChipSetting()
      unsubscribeColumnSetting()
      if (activeRepoRoot) {
        void unsubscribeFromRepo(activeRepoRoot)
        activeRepoRoot = null
      }
    },
  }
}
