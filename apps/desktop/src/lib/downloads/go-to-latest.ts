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

import { commands } from '$lib/ipc/bindings'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { revealFileInBestPane, navigateToDirInBestPane } from '$lib/file-explorer/navigation/navigate-and-select'
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
 * - `watcherUnavailable` / `downloadsDirUnresolved`: INFO toast with an
 *   "Open System Settings" action. Both states map to the same user-facing
 *   message — we can't watch Downloads, please grant FDA.
 *
 * The helper is a no-op when `explorer` is `undefined` (HMR or pre-mount).
 */
export async function goToLatestDownload(explorer: ExplorerAPI | undefined): Promise<void> {
  if (!explorer) {
    log.debug('goToLatestDownload: no explorer; skipping (HMR or pre-mount)')
    return
  }

  const result = await commands.goToLatestDownload()
  if (result.status === 'ok') {
    await revealFileInBestPane(explorer, result.data.parentDir, result.data.fileName)
    return
  }

  // Exhaustive switch on the typed enum. Never branch on the error message —
  // the `kind` discriminator is the contract.
  switch (result.error.kind) {
    case 'empty':
      await showEmptyToast(explorer)
      return
    case 'watcherUnavailable':
    case 'downloadsDirUnresolved':
      showFdaToast()
      return
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
  await revealFileInBestPane(explorer, parentDir, fileName)
}

async function showEmptyToast(explorer: ExplorerAPI): Promise<void> {
  // Resolve the Downloads dir up front so the toast's "Go to Downloads"
  // button knows where to navigate. Best-effort: if the status call fails
  // the prop closure logs and bails.
  const status = await commands.downloadsWatcherStatus()
  const downloadsDir = status.status === 'ok' ? status.data.downloadsDir : null
  // Snapshot the Downloads dir at toast-add time (it won't change), but pick the
  // target pane at CLICK time: `navigateToDirInBestPane` re-evaluates which pane
  // shows the dir then, since pane contents may shift while the toast sits there.
  const onGoToDownloads = () => {
    if (!downloadsDir) {
      log.warn('Go to Downloads pressed but Downloads dir is unresolved; no action')
      return
    }
    void navigateToDirInBestPane(explorer, downloadsDir)
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
