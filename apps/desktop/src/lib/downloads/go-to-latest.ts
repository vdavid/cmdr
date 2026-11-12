/**
 * Frontend handler for the "Go to latest download" command (⌘J, command
 * palette, and the `go_to_latest_download` MCP tool).
 *
 * Calls the typed backend IPC and branches on the typed `GoToLatestError` enum —
 * no string matching. On success, reveals the file in the best pane: if a pane
 * already shows the file's parent dir, it reuses (and focuses) that pane instead
 * of duplicating the view; otherwise it navigates the focused pane (see
 * `revealFileInBestPane`). On any error, surfaces a single INFO toast using a
 * stable dedup id so spamming ⌘J doesn't stack copies.
 */

import {
  downloadsWatcherStatus,
  goToLatestDownload as goToLatestDownloadCommand,
  trackEvent,
} from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import {
  revealFileInBestPane,
  navigateToDirInBestPane,
  resolveLocationOrToast,
} from '#lib/file-explorer/navigation/navigate-and-select.js'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

import LatestDownloadEmptyToastContent from './LatestDownloadEmptyToastContent.svelte'
import LatestDownloadFdaToastContent from './LatestDownloadFdaToastContent.svelte'
import { LATEST_DOWNLOAD_EMPTY_TOAST_ID, LATEST_DOWNLOAD_FDA_TOAST_ID } from './go-to-latest-ids'

export { LATEST_DOWNLOAD_EMPTY_TOAST_ID, LATEST_DOWNLOAD_FDA_TOAST_ID }

const log = getAppLogger('downloads')

/**
 * Go to the latest download, reusing a pane that already shows its dir.
 *
 * Contract:
 * - Success: reveal `fileName` in `parentDir` via `revealFileInBestPane` (reuse
 *   an open pane when one already shows the dir; otherwise navigate the focused
 *   pane), then select `fileName`.
 * - `empty`: INFO toast with a "Go to Downloads" action (resolves the
 *   Downloads dir via `downloadsWatcherStatus` so the action knows where to go).
 * - `watcherUnavailable` / `downloadsDirUnresolved`: one INFO toast for both,
 *   "we can't watch Downloads". On macOS it asks for Full Disk Access and offers
 *   "Open System Settings"; elsewhere it says to check the folder.
 *
 * The helper is a no-op when `explorer` is `undefined` (HMR or pre-mount).
 */
export async function goToLatestDownload(
  explorer: ExplorerAPI | undefined,
  trigger: LatestDownloadTrigger,
): Promise<void> {
  if (!explorer) {
    log.debug('goToLatestDownload: no explorer; skipping (HMR or pre-mount)')
    return
  }
  const outcome = await jumpToLatest(explorer)
  void trackEvent('download_jump_used', { trigger, outcome })
}

/**
 * What set a jump to the latest download going: `command` is ⌘J, the Go menu, and
 * the palette (one `downloads.goToLatest` command, so they don't split further);
 * `global_shortcut` is the system-wide hotkey. A required parameter, so a new
 * entry point decides what it reports. The toast's jump is `goToDownload`'s.
 */
export type LatestDownloadTrigger = 'command' | 'global_shortcut'

/**
 * How a download jump ended, for `download_jump_used`: `revealed` landed on the
 * file; `unreachable` had a file but no pane could show it; `empty` found no
 * download; `no_access` can't watch Downloads (Full Disk Access on macOS).
 */
type DownloadJumpOutcome = 'revealed' | 'unreachable' | 'empty' | 'no_access'

async function jumpToLatest(explorer: ExplorerAPI): Promise<DownloadJumpOutcome> {
  const result = await goToLatestDownloadCommand()
  if (result.status === 'ok') {
    return revealDownloadInBestPane(explorer, result.data.parentDir, result.data.fileName)
  }

  // Exhaustive switch on the typed enum. Never branch on the error message —
  // the `kind` discriminator is the contract.
  switch (result.error.kind) {
    case 'empty':
      await showEmptyToast(explorer)
      return 'empty'
    case 'watcherUnavailable':
    case 'downloadsDirUnresolved':
      showFdaToast()
      return 'no_access'
  }
}

/**
 * Go to a SPECIFIC downloaded file (parent dir + file name), bypassing the
 * latest-in-ring lookup. Reuses a pane that already shows the dir
 * (`revealFileInBestPane`) rather than duplicating the view.
 *
 * `goToLatestDownload` consults the watcher's ring + scan fallback. The
 * downloads toast already knows the path it's for; jumping to the
 * specific file matters because by the time the user clicks the toast a
 * newer download may have landed and become "latest." We want the toast
 * to take the user to the file it advertised, not whatever is most recent
 * now.
 *
 * The helper is a no-op when `explorer` is `undefined` (HMR or pre-mount).
 */
export async function goToDownload(
  explorer: ExplorerAPI | undefined,
  parentDir: string,
  fileName: string,
): Promise<void> {
  if (!explorer) {
    log.debug('goToDownload: no explorer; skipping (HMR or pre-mount)')
    return
  }
  const outcome = await revealDownloadInBestPane(explorer, parentDir, fileName)
  // The toast's own jump: it goes to the file it advertised, not the latest.
  void trackEvent('download_jump_used', { trigger: 'toast', outcome })
}

/**
 * Resolve the download's parent dir to a `Location` (volume id + path), then
 * reveal the file in the best pane. A download is local, so this resolves to a
 * real local volume; if it can't (drive gone), show the shared
 * "couldn't reach that location's drive" toast instead of navigating wrong.
 */
async function revealDownloadInBestPane(
  explorer: ExplorerAPI,
  parentDir: string,
  fileName: string,
): Promise<DownloadJumpOutcome> {
  const location = await resolveLocationOrToast(parentDir)
  if (!location) return 'unreachable'
  return (await revealFileInBestPane(explorer, location, fileName)) ? 'revealed' : 'unreachable'
}

async function showEmptyToast(explorer: ExplorerAPI): Promise<void> {
  // Resolve the Downloads dir up front so the toast's "Go to Downloads"
  // button knows where to navigate. Best-effort: if the status call fails
  // the prop closure logs and bails.
  const status = await downloadsWatcherStatus()
  const downloadsDir = status.status === 'ok' ? status.data.downloadsDir : null
  // Snapshot the Downloads dir at toast-add time (it won't change), but pick the
  // target pane at CLICK time: `navigateToDirInBestPane` re-evaluates which pane
  // shows the dir then, since pane contents may shift while the toast sits there.
  const onGoToDownloads = () => {
    if (!downloadsDir) {
      log.warn('Go to Downloads pressed but Downloads dir is unresolved; no action')
      return
    }
    void (async () => {
      // Resolve the Downloads dir's volume at click time (its drive could have
      // changed while the toast sat there), then navigate to the best pane.
      const location = await resolveLocationOrToast(downloadsDir)
      if (!location) return
      await navigateToDirInBestPane(explorer, location)
    })()
  }
  addToast(LatestDownloadEmptyToastContent, {
    id: LATEST_DOWNLOAD_EMPTY_TOAST_ID,
    level: 'info',
    toastGroup: 'downloads-go-to-latest',
    props: {
      onGoToDownloads,
    },
  })
}

function showFdaToast(): void {
  addToast(LatestDownloadFdaToastContent, {
    id: LATEST_DOWNLOAD_FDA_TOAST_ID,
    level: 'info',
    toastGroup: 'downloads-go-to-latest',
  })
}
