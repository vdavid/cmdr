/**
 * Frontend handler for the "Go to latest download" command (⌘J, command
 * palette, and the `go_to_latest_download` MCP tool).
 *
 * Calls the typed backend IPC and branches on the typed `GoToLatestError` enum —
 * no string matching. On success, navigates the focused pane to the file's
 * parent dir and moves the cursor to the file. On any error, surfaces a
 * single INFO toast using a stable dedup id so spamming ⌘J doesn't stack
 * copies.
 */

import { commands } from '$lib/ipc/bindings'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

import LatestDownloadEmptyToastContent from './LatestDownloadEmptyToastContent.svelte'
import LatestDownloadFdaToastContent from './LatestDownloadFdaToastContent.svelte'
import { LATEST_DOWNLOAD_EMPTY_TOAST_ID, LATEST_DOWNLOAD_FDA_TOAST_ID } from './go-to-latest-ids'

export { LATEST_DOWNLOAD_EMPTY_TOAST_ID, LATEST_DOWNLOAD_FDA_TOAST_ID }

const log = getAppLogger('downloads')

/**
 * Go to the latest download in the focused pane.
 *
 * Contract:
 * - Success: navigate the focused pane to `parentDir`, then select `fileName`.
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
    await navigateToDownloadFile(explorer, result.data.parentDir, result.data.fileName)
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
 * Go to a SPECIFIC downloaded file (parent dir + file name) in the focused
 * pane, bypassing the latest-in-ring lookup.
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
  await navigateToDownloadFile(explorer, parentDir, fileName)
}

async function navigateToDownloadFile(explorer: ExplorerAPI, parentDir: string, fileName: string): Promise<void> {
  const pane = explorer.getFocusedPane()
  // `navigateToPath` returns a sync error string when navigation can't even
  // start (snapshot pane on a missing volume, etc.); otherwise it returns a
  // Promise that settles when the listing completes. We await the Promise
  // but report-and-bail on the sync-error string — without the listing
  // settled, `moveCursor` would race against an empty cache.
  const navResult = explorer.navigateToPath(pane, parentDir)
  if (typeof navResult === 'string') {
    log.warn('goToDownload: navigateToPath refused {pane} {parentDir}: {reason}', {
      pane,
      parentDir,
      reason: navResult,
    })
    return
  }
  await navResult
  await explorer.moveCursor(pane, fileName)
}

async function showEmptyToast(explorer: ExplorerAPI): Promise<void> {
  // Resolve the Downloads dir up front so the toast's "Go to Downloads"
  // button knows where to navigate. Best-effort: if the status call fails
  // the prop closure logs and bails.
  const status = await commands.downloadsWatcherStatus()
  const downloadsDir = status.status === 'ok' ? status.data.downloadsDir : null
  // Snapshot the focused pane + Downloads dir at toast-add time so a later
  // pane focus change doesn't redirect the action somewhere unexpected.
  const focusedPane = explorer.getFocusedPane()
  const onGoToDownloads = () => {
    if (!downloadsDir) {
      log.warn('Go to Downloads pressed but Downloads dir is unresolved; no action')
      return
    }
    const result = explorer.navigateToPath(focusedPane, downloadsDir)
    if (typeof result === 'string') {
      log.warn('Go to Downloads: navigateToPath refused: {reason}', { reason: result })
    }
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
