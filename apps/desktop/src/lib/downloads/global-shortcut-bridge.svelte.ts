/**
 * Frontend bridge for the global reveal hotkey (default ⌃⌥⌘J).
 *
 * Subscribes ONCE to the backend `global-shortcut-fired` event. Every fire:
 *
 * 1. Reads the current `acknowledged` flag. If `false`, flips it to `true`
 *    AND opens the first-trigger warn toast. If `true`, skips the toast.
 * 2. Calls `revealLatestDownload(explorer)` so the user lands on the file.
 *
 * Mounted from `routes/(main)/+page.svelte` alongside the M5 event bridge.
 * The unsubscribe is returned so the layout can clean up on destroy.
 *
 * ## Why flip `acknowledged` BEFORE opening the toast (not inside the toast)
 *
 * If `acknowledged` were flipped from inside the toast's primary button,
 * pressing the hotkey twice quickly in a row would queue two toasts (the
 * second fire would still see `acknowledged === false` because the first
 * toast hasn't been confirmed yet). Flipping early collapses the race.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'
import { getSetting } from '$lib/settings'
import { getAppLogger } from '$lib/logging/logger'
import GlobalShortcutWarnToastContent from './GlobalShortcutWarnToastContent.svelte'
import { revealLatestDownload } from './reveal'
import {
  GLOBAL_REVEAL_ACKNOWLEDGED_KEY,
  GLOBAL_REVEAL_BINDING_KEY,
  setGlobalRevealAcknowledged,
} from './global-shortcut-setting'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

const log = getAppLogger('downloads')

export const GLOBAL_SHORTCUT_FIRED_EVENT = 'global-shortcut-fired'
const WARN_TOAST_ID = 'downloads-global-shortcut-warn'

/**
 * Mount the bridge. Returns an unsubscribe function — call it from the
 * page's `onDestroy`.
 */
export async function startGlobalShortcutBridge(explorer: ExplorerAPI | undefined): Promise<UnlistenFn> {
  const unlisten = await listen<unknown>(GLOBAL_SHORTCUT_FIRED_EVENT, () => {
    void handleFired(explorer)
  })
  log.debug('Global shortcut bridge mounted')
  return unlisten
}

async function handleFired(explorer: ExplorerAPI | undefined): Promise<void> {
  // Read the snapshot eagerly so the toast carries the binding string that
  // was active at THIS moment, even if the user remaps mid-flight.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- registry key
  const acknowledged = getSetting(GLOBAL_REVEAL_ACKNOWLEDGED_KEY as any) as boolean
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- registry key
  const binding = getSetting(GLOBAL_REVEAL_BINDING_KEY as any) as string

  if (!acknowledged) {
    // Flip first to collapse the back-to-back-press race. The toast itself
    // doesn't re-write this bit.
    setGlobalRevealAcknowledged(true)
    addToast(GlobalShortcutWarnToastContent, {
      id: WARN_TOAST_ID,
      level: 'warn',
      dismissal: 'persistent',
      props: { binding },
    })
  }

  await revealLatestDownload(explorer)
}
