/**
 * Frontend handler for the "Reveal latest download" command (⌘J, command
 * palette, and the `reveal_latest_download` MCP tool).
 *
 * Calls the typed M2b IPC and branches on the typed `RevealError` enum — no
 * string matching. On success, navigates the focused pane to the file's
 * parent dir and moves the cursor to the file. On any error, surfaces a
 * single INFO toast using a stable dedup id so spamming ⌘J doesn't stack
 * copies.
 */

import { commands } from '$lib/ipc/bindings'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

import RevealEmptyToastContent from './RevealEmptyToastContent.svelte'
import RevealFdaToastContent from './RevealFdaToastContent.svelte'
import { REVEAL_EMPTY_TOAST_ID, REVEAL_FDA_TOAST_ID } from './reveal-ids'

export { REVEAL_EMPTY_TOAST_ID, REVEAL_FDA_TOAST_ID }

const log = getAppLogger('downloads')

/**
 * Reveal the latest download in the focused pane.
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
export async function revealLatestDownload(explorer: ExplorerAPI | undefined): Promise<void> {
  if (!explorer) {
    log.debug('revealLatestDownload: no explorer; skipping (HMR or pre-mount)')
    return
  }

  const result = await commands.revealLatestDownload()
  if (result.status === 'ok') {
    await navigateToRevealedFile(explorer, result.data.parentDir, result.data.fileName)
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
 * Reveal a SPECIFIC downloaded file (parent dir + file name) in the focused
 * pane, bypassing the latest-in-ring lookup.
 *
 * `revealLatestDownload` consults the watcher's ring + scan fallback. The
 * downloads toast (M5) already knows the path it's for; revealing the
 * specific file matters because by the time the user clicks the toast a
 * newer download may have landed and become "latest." We want the toast
 * to take the user to the file it advertised, not whatever is most recent
 * now.
 *
 * The helper is a no-op when `explorer` is `undefined` (HMR or pre-mount).
 */
export async function revealPath(
  explorer: ExplorerAPI | undefined,
  parentDir: string,
  fileName: string,
): Promise<void> {
  if (!explorer) {
    log.debug('revealPath: no explorer; skipping (HMR or pre-mount)')
    return
  }
  await navigateToRevealedFile(explorer, parentDir, fileName)
}

async function navigateToRevealedFile(explorer: ExplorerAPI, parentDir: string, fileName: string): Promise<void> {
  const pane = explorer.getFocusedPane()
  // `navigateToPath` returns a sync error string when navigation can't even
  // start (snapshot pane on a missing volume, etc.); otherwise it returns a
  // Promise that settles when the listing completes. We await the Promise
  // but report-and-bail on the sync-error string — without the listing
  // settled, `moveCursor` would race against an empty cache.
  const navResult = explorer.navigateToPath(pane, parentDir)
  if (typeof navResult === 'string') {
    log.warn('revealLatestDownload: navigateToPath refused {pane} {parentDir}: {reason}', {
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
  addToast(RevealEmptyToastContent, {
    id: REVEAL_EMPTY_TOAST_ID,
    level: 'info',
    toastGroup: 'downloads-reveal',
    props: {
      onGoToDownloads,
    },
  })
}

function showFdaToast(): void {
  addToast(RevealFdaToastContent, {
    id: REVEAL_FDA_TOAST_ID,
    level: 'info',
    toastGroup: 'downloads-reveal',
  })
}
