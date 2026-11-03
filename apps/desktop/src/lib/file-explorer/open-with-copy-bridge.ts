/**
 * "Open with" copy notice bridge.
 *
 * A file inside an archive (or a repo's `.git` snapshot, or a restic backup) has no file of its own, so
 * "Open with" copies it out before launching the app. Two backend events become toasts:
 *
 * - `open-with-copy-refused`: the copy couldn't be made (too big, a locked or damaged
 *   archive), so no app opened; without the warning the click would do nothing at all.
 * - `open-with-copy-launched`: the app got a read-only copy. Nothing else tells the
 *   person that what they save there won't go back into the original.
 *
 * A third event rides along, `open-with-not-on-this-mac`: the row's volume hands out
 * paths no other app can open (a phone, a server, a direct SMB share whose macOS mount
 * went away), so no app launched. Rust checks it in `launch_with`, because only the
 * volume knows when a share's mount is gone.
 *
 * The click lands on the native context menu, which Rust handles end to end
 * (`menu/open_with.rs::launch_with`), so the frontend never sees the launch: the
 * events are its only way to hear about it. Keyed per file, so clicking again replaces
 * the notice instead of stacking a copy. Mounted from `routes/(main)/window-services.ts`
 * beside the other event bridges.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatByteSize } from '#lib/units/index.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { onOpenWithCopyLaunched, onOpenWithCopyRefused, onOpenWithNotOnThisMac } from '#lib/tauri-commands/index.js'
import type {
  OpenWithCopyLaunched,
  OpenWithCopyRefused,
  OpenWithCopySource,
  OpenWithNotOnThisMac,
} from '#lib/ipc/bindings.js'
import { viewShortcut } from './pane/default-app-open'
import type { MessageKey } from '#lib/intl/keys.gen.js'

const log = getAppLogger('fileExplorer')

/** Long enough to read a two-sentence notice that says what to do next. */
const NOTICE_TIMEOUT_MS = 10_000

/** Mounts all three listeners. Returns their unsubscribe. */
export async function startOpenWithCopyBridge(): Promise<UnlistenFn> {
  const unlistenRefused = await onOpenWithCopyRefused(raiseNotice)
  const unlistenLaunched = await onOpenWithCopyLaunched(raiseLaunchedNotice)
  const unlistenNotOnThisMac = await onOpenWithNotOnThisMac(raiseNotOnThisMacNotice)
  log.debug('Open-with copy notice bridge mounted')
  return () => {
    unlistenRefused()
    unlistenLaunched()
    unlistenNotOnThisMac()
  }
}

function raiseNotOnThisMacNotice({ appName }: OpenWithNotOnThisMac): void {
  log.info('Open with {appName} didn’t launch: the file has no path another app can open', { appName })
  addToast(tString('fileExplorer.openWith.notOnThisMac', { appName, viewKey: viewShortcut() }), {
    level: 'info',
    timeoutMs: NOTICE_TIMEOUT_MS,
    id: 'open-with-not-on-this-mac',
  })
}

function raiseLaunchedNotice({ fileName, appName, copyCount }: OpenWithCopyLaunched): void {
  log.info('Open with {appName} launched on {copyCount} read-only copies', { appName, copyCount })
  addToast(
    tString('fileExplorer.openWith.readOnlyCopy', {
      fileName,
      appName,
      count: copyCount,
      countText: formatInteger(copyCount),
    }),
    { level: 'info', timeoutMs: NOTICE_TIMEOUT_MS, id: `open-with-copy-launched-${fileName}` },
  )
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
      // from a repo's history or a backup snapshot too, neither "inside the archive".
      return tString(tooLargeKey(source), { fileName, appName, limit: formatByteSize(reason.cap) })
    case 'notEnoughSpace':
      return tString('fileExplorer.openWith.copyRefused.notEnoughSpace', {
        fileName,
        appName,
        size: formatByteSize(reason.needed),
      })
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

/** Where a too-big file sits, in the words its toast uses. */
function tooLargeKey(source: OpenWithCopySource): MessageKey {
  switch (source) {
    case 'archive':
      return 'fileExplorer.openWith.copyRefused.tooLarge'
    case 'repoHistory':
      return 'fileExplorer.openWith.copyRefused.tooLargeInRepoHistory'
    case 'backup':
      return 'fileExplorer.openWith.copyRefused.tooLargeInBackup'
    default: {
      const unhandled: never = source
      return unhandled
    }
  }
}
