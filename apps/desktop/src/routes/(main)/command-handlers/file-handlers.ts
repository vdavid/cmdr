/**
 * File-action handlers: the viewer / rename / copy / move / new-folder/file /
 * delete dialog openers, the MCP `dialog.confirm`, and the
 * get-entry-under-cursor-then-act arms (edit / show in Finder / copy path /
 * copy filename / get info / the cloud offline pair). The repeated "read the
 * entry under the cursor, act on it if present" shape is the `withEntryUnderCursor`
 * helper, so the cursor read happens once per arm.
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
import { getFocusedPanePath, getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import { pathInsideArchive } from '$lib/file-explorer/pane/volume-capabilities'
import { tString } from '$lib/intl/messages.svelte'
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

  'file.edit': (hctx) => withEntryUnderCursor(hctx, (entry) => openInEditor(entry.path)),

  'file.copy': ({ explorerRef, dispatchArgs }) => {
    // Arg-less from the F-bar / palette / keyboard (open the dialog with no
    // preset); the MCP `copy` tool may pass `{ autoConfirm, onConflict }` to
    // pre-answer the conflict policy. `dispatchArgs` is `undefined` in the
    // arg-less case, so the openers default both.
    const copyArgs = dispatchArgs as CommandArgs['file.copy'] | undefined
    void explorerRef?.openCopyDialog(copyArgs?.autoConfirm, copyArgs?.onConflict, copyArgs?.mcpRequestId)
  },

  'file.move': ({ explorerRef, dispatchArgs }) => {
    const moveArgs = dispatchArgs as CommandArgs['file.move'] | undefined
    void explorerRef?.openMoveDialog(moveArgs?.autoConfirm, moveArgs?.onConflict, moveArgs?.mcpRequestId)
  },

  'file.compress': ({ explorerRef, dispatchArgs }) => {
    const compressArgs = dispatchArgs as CommandArgs['file.compress'] | undefined
    void explorerRef?.openCompressDialog(
      compressArgs?.autoConfirm,
      compressArgs?.onConflict,
      compressArgs?.mcpRequestId,
    )
  },

  'file.newFolder': ({ explorerRef, dispatchArgs }) => {
    // Arg-less from F7 / the palette; the MCP `mkdir` tool may pass `{ name }` to
    // prefill the dialog and `{ pane }` to target a specific pane. (autoConfirm
    // creates directly in Rust, never reaching here.)
    const args = dispatchArgs as CommandArgs['file.newFolder'] | undefined
    void explorerRef?.openNewFolderDialog(args?.name, args?.pane)
  },

  'file.newFile': ({ explorerRef, dispatchArgs }) => {
    const args = dispatchArgs as CommandArgs['file.newFile'] | undefined
    void explorerRef?.openNewFileDialog(args?.name, args?.pane)
  },

  'file.delete': ({ explorerRef, dispatchArgs }) => {
    // The MCP `delete` tool may pass `permanent` (from its `mode`); F8 omits it
    // (trash-default). The dialog still clamps to permanent on no-trash volumes.
    const deleteArgs = dispatchArgs as CommandArgs['file.delete'] | undefined
    void explorerRef?.openDeleteDialog(
      deleteArgs?.permanent ?? false,
      deleteArgs?.autoConfirm,
      deleteArgs?.mcpRequestId,
    )
  },

  'file.deletePermanently': ({ explorerRef }) => {
    void explorerRef?.openDeleteDialog(true)
  },

  'dialog.confirm': ({ explorerRef, dispatchArgs }) => {
    // MCP `dialog confirm` tool: programmatically confirm an already-open
    // transfer/delete dialog.
    const { type, onConflict } = dispatchArgs as CommandArgs['dialog.confirm']
    explorerRef?.confirmDialog(type, onConflict)
  },

  'file.showInFinder': (hctx) => withEntryUnderCursor(hctx, (entry) => showInFinder(entry.path)),

  'file.copyPath': (hctx) => withEntryUnderCursor(hctx, (entry) => copyToClipboard(entry.path)),

  'file.copyCurrentDirectoryPath': async () => {
    const currentPath = getFocusedPanePath()
    if (currentPath) {
      await copyToClipboard(currentPath)
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
      return
    }
    armQuickLookDispatchGuard()
    if (quickLookState.isOpen) {
      quickLookState.isOpen = false
      await quickLookClose()
      return
    }
    const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
    if (!entryUnderCursor) return
    // Quick Look can't preview a file INSIDE an archive: the inner path isn't a
    // real file on disk, so the panel would open blank. No-op — consistent with
    // how Quick Look already skips non-local volumes; F3 (viewer temp-extract) is
    // the preview path inside a zip. Return BEFORE flipping `isOpen` so state stays
    // consistent (no panel opened).
    if (pathInsideArchive(entryUnderCursor.path)) return
    const volumeId = getFocusedPaneVolumeId()
    // Optimistically flip `isOpen` before the IPC: AppKit returns from
    // `makeKeyAndOrderFront:` synchronously and the panel is up by the time
    // the IPC resolves, but the optimistic flip means a second Shift+Space
    // press immediately after the first reads the right state.
    quickLookState.isOpen = true
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
