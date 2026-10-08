/**
 * "Open with" refusal notice bridge.
 *
 * Turns the backend's `open-with-copy-refused` event into a warning toast. A file
 * inside an archive (or a repo's `.git` snapshot) has no file of its own, so "Open
 * with" copies it out before launching the app; when that copy can't be made (too
 * big, a locked or damaged archive), no app opens, and without this the click
 * would do nothing at all.
 *
 * The click lands on the native context menu, which Rust handles end to end
 * (`menu/open_with.rs::launch_with`), so the frontend never sees the launch: the
 * event is its only way to hear about it. Keyed per file, so clicking again replaces
 * the notice instead of stacking a copy. Mounted from `routes/(main)/window-services.ts`
 * beside the other event bridges.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { tString } from '$lib/intl/messages.svelte'
import { formatByteSize } from '$lib/units'
import { onOpenWithCopyRefused } from '$lib/tauri-commands'
import type { OpenWithCopyRefused } from '$lib/ipc/bindings'

const log = getAppLogger('fileExplorer')

/** Long enough to read a two-sentence notice that says what to do next. */
const NOTICE_TIMEOUT_MS = 10_000

/** Mounts the listener. Returns its unsubscribe. */
export async function startOpenWithRefusedBridge(): Promise<UnlistenFn> {
  const unlisten = await onOpenWithCopyRefused(raiseNotice)
  log.debug('Open-with refusal notice bridge mounted')
  return unlisten
}

function raiseNotice(payload: OpenWithCopyRefused): void {
  log.info('Open with {appName} didn’t launch: couldn’t copy {fileName} out ({reason})', {
    appName: payload.appName,
    fileName: payload.fileName,
    reason: payload.reason.kind,
  })
  addToast(noticeText(payload), {
    level: 'warn',
    timeoutMs: NOTICE_TIMEOUT_MS,
    id: `open-with-copy-refused-${payload.fileName}`,
  })
}

function noticeText({ fileName, appName, reason, source }: OpenWithCopyRefused): string {
  switch (reason.kind) {
    case 'tooLarge':
      // The archive reasons below only ever come from an archive; this one can come
      // from a repo's history snapshot too, which isn't "inside the archive".
      return tString(
        source === 'repoHistory'
          ? 'fileExplorer.openWith.copyRefused.tooLargeInRepoHistory'
          : 'fileExplorer.openWith.copyRefused.tooLarge',
        { fileName, appName, limit: formatByteSize(reason.cap) },
      )
    case 'needsPassword':
      return tString('fileExplorer.openWith.copyRefused.needsPassword', { fileName, appName })
    case 'archiveUnreadable':
      return tString('fileExplorer.openWith.copyRefused.archiveUnreadable', { fileName, appName })
    case 'unreadable':
      return tString('fileExplorer.openWith.copyRefused.unreadable', { fileName, appName })
    default: {
      const unhandled: never = reason
      return unhandled
    }
  }
}
