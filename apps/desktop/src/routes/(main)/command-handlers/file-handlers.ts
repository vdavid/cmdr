/**
 * File-action handlers: the viewer / rename / copy / move / new-folder/file /
 * delete dialog openers, the dialog-less Duplicate, the MCP `dialog.confirm`, and the
 * get-entry-under-cursor-then-act arms (edit / show in Finder / copy filename /
 * get info / the cloud offline pair). The repeated "read the entry under the
 * cursor, act on it if present" shape is the `withEntryUnderCursor` helper, so the
 * cursor read happens once per arm. The two copy-a-path arms sit outside it: they
 * accept the `..` row (as the pane's own directory) and share `copyPathAndAnnounce`.
 */
import {
  showInFinder,
  copyToClipboard,
  quickLookOpen,
  quickLookClose,
  getInfo,
  openInEditor,
  cloudMakeAvailableOffline,
  cloudRemoveDownload,
} from '$lib/tauri-commands'
import {
  quickLookState,
  quickLookDispatchGuardJustFired,
  armQuickLookDispatchGuard,
} from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import { addToast } from '$lib/ui/toast'
import CopiedPathToastContent from '$lib/file-explorer/CopiedPathToastContent.svelte'
import { getFocusedPanePath, getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import { capabilitiesFor, pathInsideArchive } from '$lib/file-explorer/pane/volume-capabilities'
import { resolveTerminalFolder } from '$lib/open-terminal/terminal-target'
import { openTerminalHereForFolder } from '$lib/open-terminal/open-terminal-here'
import { tString } from '$lib/intl/messages.svelte'
import { trackEvent } from '$lib/tauri-commands'
import type { CommandArgs } from '$lib/commands'
import type { CommandHandlerContext, CommandHandlerRecord } from './types'

/** The file entry the focused pane's cursor sits on (path + filename). */
type EntryUnderCursor = { path: string; filename: string }

/**
 * Reads the entry under the focused pane's cursor ONCE and, when present, runs
 * `fn` with it. The cloud and `file.*`-under-cursor arms all share this shape;
 * the helper keeps the single-read discipline inside the module (no arm re-reads
 * `getFileAndPathUnderCursor()`). Returns `fn`'s result so awaiting arms can
 * propagate completion (and rejections) to the dispatch promise.
 */
function withEntryUnderCursor(
  { explorerRef }: CommandHandlerContext,
  fn: (entry: EntryUnderCursor) => void | Promise<void>,
): void | Promise<void> {
  const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
  if (entryUnderCursor) {
    return fn(entryUnderCursor)
  }
}

/**
 * Max width of the copied-path toast. Wider than the 360 default so a typical
 * home-directory path fits on one or two lines; capped by the toast container's
 * own 440. The toast still shrinks to its content, so a short path doesn't get a
 * wide box.
 */
const COPIED_PATH_TOAST_WIDTH_PX = 432

/**
 * Puts `path` on the clipboard and confirms it with a transient info toast that
 * shows what landed there.
 *
 * The toast joins a one-slot group rather than reusing a fixed toast id: dedup by
 * id replaces content and level in place but NOT props, so a second copy would
 * re-show the first path. Group eviction retires the old toast and pushes a fresh
 * one, which also restarts the auto-dismiss timer.
 */
async function copyPathAndAnnounce(path: string): Promise<void> {
  await copyToClipboard(path)
  addToast(CopiedPathToastContent, {
    level: 'info',
    props: { path },
    widthPx: COPIED_PATH_TOAST_WIDTH_PX,
    toastGroup: 'copied-path',
    maxInGroup: 1,
  })
}

export const fileHandlers = {
  'file.view': ({ explorerRef }) => {
    void explorerRef?.openViewerForCursor()
  },

  'file.rename': ({ explorerRef, dispatchArgs }) => {
    // Arg-less from F2 / the palette (seed the current name); the MCP `rename`
    // tool passes `{ initialName, expectedName }` to seed a proposed name and pin
    // activation to the target row.
    const renameArgs = dispatchArgs as CommandArgs['file.rename'] | undefined
    explorerRef?.startRename(renameArgs)
  },

  'file.edit': (hctx) =>
    withEntryUnderCursor(hctx, (entry) => {
      // F4 hands the file to the OS's text editor (`open -t`), so there's nothing
      // downstream of this to count it. No props: the only honest thing this
      // knows is that somebody pressed it, and the file's name and extension are
      // exactly what must never cross.
      void trackEvent('editor_opened', {})
      return openInEditor(entry.path)
    }),

  'file.copy': ({ explorerRef, dispatchArgs }) => {
    // Arg-less from the F-bar / palette / keyboard (open the dialog with no
    // preset); the MCP `copy` tool may pass `{ autoConfirm, onConflict, initiator }`
    // to pre-answer the conflict policy and tag provenance. `dispatchArgs` is
    // `undefined` in the arg-less case, so the openers default them all.
    const copyArgs = dispatchArgs as CommandArgs['file.copy'] | undefined
    void explorerRef?.openCopyDialog(copyArgs)
  },

  'file.duplicate': ({ explorerRef }) => {
    // Arg-less by design: Duplicate has no destination to choose and no conflict
    // policy to pre-answer, so there's nothing an MCP payload could say.
    void explorerRef?.duplicateInPlace()
  },

  'file.move': ({ explorerRef, dispatchArgs }) => {
    const moveArgs = dispatchArgs as CommandArgs['file.move'] | undefined
    void explorerRef?.openMoveDialog(moveArgs)
  },

  'file.compress': ({ explorerRef, dispatchArgs }) => {
    const compressArgs = dispatchArgs as CommandArgs['file.compress'] | undefined
    void explorerRef?.openCompressDialog(compressArgs)
  },

  'file.newFolder': ({ explorerRef, dispatchArgs }) => {
    // Arg-less from F7 / the palette; the MCP `mkdir` tool may pass `{ name }` to
    // prefill the dialog and `{ pane }` to target a specific pane, plus `{ initiator }`
    // to tag provenance. (autoConfirm creates directly in Rust, never reaching here.)
    const args = dispatchArgs as CommandArgs['file.newFolder'] | undefined
    void explorerRef?.openNewFolderDialog(args?.name, args?.pane, args?.initiator)
  },

  'file.newFile': ({ explorerRef, dispatchArgs }) => {
    const args = dispatchArgs as CommandArgs['file.newFile'] | undefined
    void explorerRef?.openNewFileDialog(args?.name, args?.pane, args?.initiator)
  },

  'file.delete': ({ explorerRef, dispatchArgs }) => {
    // The MCP `delete` tool may pass `permanent` (from its `mode`); F8 omits it
    // (trash-default). The dialog still clamps to permanent on no-trash volumes.
    const deleteArgs = dispatchArgs as CommandArgs['file.delete'] | undefined
    void explorerRef?.openDeleteDialog({
      permanent: deleteArgs?.permanent ?? false,
      autoConfirm: deleteArgs?.autoConfirm,
      mcpRequestId: deleteArgs?.mcpRequestId,
      initiator: deleteArgs?.initiator,
    })
  },

  'file.deletePermanently': ({ explorerRef }) => {
    void explorerRef?.openDeleteDialog({ permanent: true })
  },

  'dialog.confirm': ({ explorerRef, dispatchArgs }) => {
    // MCP `dialog confirm` tool: programmatically confirm an already-open
    // transfer/delete dialog.
    const { type, onConflict } = dispatchArgs as CommandArgs['dialog.confirm']
    explorerRef?.confirmDialog(type, onConflict)
  },

  'file.showInFinder': (hctx) => withEntryUnderCursor(hctx, (entry) => showInFinder(entry.path)),

  'file.openTerminalHere': ({ explorerRef }) => {
    // Not `withEntryUnderCursor`: this acts on a FOLDER, and every cursor state
    // resolves to one (a folder row gives itself; a file, `..`, or an empty pane
    // gives the pane's own). `resolveTerminalFolder` owns those rules.
    const volumeId = getFocusedPaneVolumeId()
    const folder = resolveTerminalFolder({
      panePath: getFocusedPanePath(),
      // The VOLUME's kind, never `capabilitiesForPane`: an archive pane's
      // kind-from-path would hide the drive the archive actually lives on.
      volumeKind: capabilitiesFor(volumeId).kind,
      cursorEntry: explorerRef?.getCursorRowForTerminal() ?? null,
    })
    void openTerminalHereForFolder({ folder, volumeId })
  },

  'file.copyPath': async ({ explorerRef }) => {
    // Not `withEntryUnderCursor`: on the `..` row this copies the pane's OWN
    // directory, where every other under-cursor arm treats `..` as "no entry".
    const path = explorerRef?.getPathToCopyUnderCursor()
    if (path) {
      await copyPathAndAnnounce(path)
    }
  },

  'file.copyCurrentDirectoryPath': async () => {
    const currentPath = getFocusedPanePath()
    if (currentPath) {
      await copyPathAndAnnounce(currentPath)
    }
  },

  'file.copyFilename': (hctx) => withEntryUnderCursor(hctx, (entry) => copyToClipboard(entry.filename)),

  'file.quickLook': async ({ explorerRef }) => {
    // Shift+Space toggles. The panel close path (✕, Esc, our `quickLookClose`
    // call below) all converge on a `quick-look-closed` event that flips
    // `isOpen` back to false in the state singleton, so the next press opens.
    //
    // Race guard: every Shift+Space keypress fires this case twice — once via
    // AppKit's menu accelerator (`on_menu_event` → `execute-command` event)
    // and once via WKWebView's keydown → centralized JS shortcut dispatch.
    // Without the guard, the second fire toggles the panel back. The guard
    // also covers the panel-key Shift+Space-from-listener path (which arms
    // it before flipping `isOpen`).
    if (quickLookDispatchGuardJustFired()) {
      // The duplicate fire of ONE keypress. Counting it would double every
      // number this event produces.
      return
    }
    armQuickLookDispatchGuard()
    if (quickLookState.isOpen) {
      quickLookState.isOpen = false
      await quickLookClose()
      void trackEvent('quick_look_used', { outcome: 'closed' })
      return
    }
    const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
    if (!entryUnderCursor) {
      void trackEvent('quick_look_used', { outcome: 'noTarget' })
      return
    }
    // Quick Look can't preview a file INSIDE an archive: the inner path isn't a
    // real file on disk, so the panel would open blank. No-op — consistent with
    // how Quick Look already skips non-local volumes; F3 (viewer temp-extract) is
    // the preview path inside a zip. Return BEFORE flipping `isOpen` so state stays
    // consistent (no panel opened).
    if (pathInsideArchive(entryUnderCursor.path)) {
      // The one gate in front of Quick Look. Counted so a low `opened` number can
      // be told apart from people reaching for it where it can't work; without
      // the refusal, a zero is unreadable (`analytics/DETAILS.md` § Reading a zero).
      void trackEvent('quick_look_used', { outcome: 'insideArchive' })
      return
    }
    const volumeId = getFocusedPaneVolumeId()
    // Optimistically flip `isOpen` before the IPC: AppKit returns from
    // `makeKeyAndOrderFront:` synchronously and the panel is up by the time
    // the IPC resolves, but the optimistic flip means a second Shift+Space
    // press immediately after the first reads the right state.
    quickLookState.isOpen = true
    void trackEvent('quick_look_used', { outcome: 'opened' })
    await quickLookOpen(entryUnderCursor.path, volumeId)
  },

  'file.getInfo': (hctx) => withEntryUnderCursor(hctx, (entry) => getInfo(entry.path)),

  'cloud.makeOffline': (hctx) =>
    withEntryUnderCursor(hctx, async (entry) => {
      try {
        await cloudMakeAvailableOffline(entry.path)
      } catch (e) {
        addToast(tString('commands.handler.cloudDownloadFailed', { detail: String(e) }), { level: 'error' })
      }
    }),

  'cloud.removeDownload': (hctx) =>
    withEntryUnderCursor(hctx, async (entry) => {
      try {
        await cloudRemoveDownload(entry.path)
      } catch (e) {
        addToast(tString('commands.handler.cloudRemoveDownloadFailed', { detail: String(e) }), { level: 'error' })
      }
    }),
} satisfies Partial<CommandHandlerRecord>
