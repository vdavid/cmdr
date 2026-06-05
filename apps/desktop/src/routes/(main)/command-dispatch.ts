/**
 * Command dispatch: maps command IDs from the command palette, keyboard shortcuts,
 * and menu actions to concrete app actions.
 */

import {
  openExternalUrl,
  showInFinder,
  copyToClipboard,
  quickLookOpen,
  quickLookClose,
  getInfo,
  openInEditor,
  syncMenuShowHidden,
  readClipboardText,
  cloudMakeAvailableOffline,
  cloudRemoveDownload,
} from '$lib/tauri-commands'
import {
  quickLookState,
  quickLookDispatchGuardJustFired,
  armQuickLookDispatchGuard,
} from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import { invoke } from '@tauri-apps/api/core'
import { addToast } from '$lib/ui/toast'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from '$lib/search/capabilities'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
import { runMenuTriggeredCheck } from '$lib/updates/updater.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { goToLatestDownload } from '$lib/downloads/go-to-latest'
import { getFocusedPanePath, getFocusedPaneVolumeId } from '$lib/file-explorer/pane/focused-pane-reads'
import type { CommandId, CommandArgs, CommandDispatchArgs } from '$lib/commands'
import type { ExplorerAPI } from './explorer-api'

const log = getAppLogger('user-action')

/** Callbacks for toggling dialog visibility from command dispatch */
export interface CommandDispatchDialogs {
  showCommandPalette: (show: boolean) => void
  showSearchDialog: (show: boolean) => void
  /**
   * Opens or closes the "Go to path" dialog. The open path is guarded against
   * the menu double-dispatch (a ⌘G accelerator can fire both the menu event and
   * the JS keydown); the callback no-ops when already open.
   */
  showGoToPathDialog: (show: boolean) => void
  showAboutWindow: (show: boolean) => void
  showLicenseKeyDialog: (show: boolean) => void
  /**
   * Opens or closes the Selection dialog. `'add'` opens "Select files…",
   * `'remove'` opens "Deselect files…", `null` closes.
   */
  showSelectionDialog: (mode: 'add' | 'remove' | null) => void
  /**
   * Opens the onboarding wizard for re-entry from the `Cmdr > Onboarding…`
   * menu item or the `cmdr.openOnboarding` command palette command. No-op when
   * the wizard is already open.
   */
  openOnboarding: () => void
}

export interface CommandDispatchContext {
  getExplorer: () => ExplorerAPI | undefined
  dialogs: CommandDispatchDialogs
}

/**
 * Returns the closest selectable-text container (e.g. `ErrorPane`) the current text
 * selection sits in, or `null` if the selection isn't inside one. Even a collapsed
 * selection counts (the user clicked into the region), so ⌘A works without prior
 * highlighting. Add `data-text-region` to opt new components into this routing.
 */
function activeTextRegion(): Element | null {
  const sel = window.getSelection()
  const anchor = sel?.anchorNode
  if (!anchor) return null
  const el = anchor.nodeType === Node.ELEMENT_NODE ? (anchor as Element) : anchor.parentElement
  return el?.closest('.error-pane, [data-text-region]') ?? null
}

/**
 * Returns `true` (and surfaces a toast) when `commandId` is a destination-side
 * action that the focused pane's volume can't satisfy because it's a
 * `search-results://` virtual pane. The capability flag set is documented in
 * `lib/search/capabilities.ts`. Source-side actions (copy/move/delete with the
 * snapshot as the source) stay enabled.
 *
 * Per the M8c plan: menu paths are disabled at the source (F-bar buttons,
 * context-menu items), so this guard exists for the shortcut-driven path
 * (⌘V paste, F7 mkdir, etc.) that bypasses the UI entirely. The toast is
 * the LAST RESORT — it's there so the user isn't left wondering whether
 * the keystroke registered.
 */
function blockedBySearchResultsPane(commandId: CommandId, explorer: ExplorerAPI | undefined): boolean {
  if (!explorer) return false
  if (getFocusedPaneVolumeId() !== 'search-results') return false

  const isBlocked =
    commandId === 'edit.paste' ||
    commandId === 'edit.pasteAsMove' ||
    commandId === 'file.newFolder' ||
    commandId === 'file.newFile' ||
    commandId === 'file.rename'
  if (!isBlocked) return false

  addToast(SEARCH_RESULTS_NOT_A_FOLDER_TOAST, { level: 'info' })
  return true
}

/**
 * Intercepts text-region shortcuts (⌘C, ⌘A) BEFORE the dispatcher logs or records
 * the action, so selecting text in the ErrorPane and copying it doesn't pollute the
 * user-action log used for rollback context, and doesn't fire file-scope side
 * effects (copy files, select all files). Returns `true` if the shortcut was handled.
 *
 * For `edit.copy` we only intercept when the selection is non-collapsed (something is
 * actually selected); otherwise we fall through so the file copy path can run.
 */
function handleTextRegionShortcut(commandId: CommandId): boolean {
  if (commandId !== 'edit.copy' && commandId !== 'selection.selectAll') return false
  const region = activeTextRegion()
  if (!region) return false

  if (commandId === 'edit.copy') {
    const text = window.getSelection()?.toString() ?? ''
    if (!text) return false
    void navigator.clipboard.writeText(text)
    return true
  }

  // selection.selectAll: replace the current selection with the whole region.
  // Includes hidden content inside collapsed <details>, which is what the user
  // actually wants when copying error context (technical details included).
  const range = document.createRange()
  range.selectNodeContents(region)
  const sel = window.getSelection()
  sel?.removeAllRanges()
  sel?.addRange(range)
  return true
}

/**
 * Shows a transient toast confirming a zoom change. Surfaces the reset shortcut
 * (or menu path if no shortcut is bound) so users who hit ⌘+/⌘- by accident
 * know how to get back to 100%.
 */
function showZoomToast(oldSize: number, newSize: number): void {
  if (oldSize === newSize) return

  const resetShortcut = getEffectiveShortcuts('view.zoom.set100')[0]
  const resetHint = resetShortcut
    ? `You can reset the zoom level to 100% by ${resetShortcut}.`
    : 'You can reset the zoom level to 100% at View > Zoom > 100%.'

  let message: string
  if (newSize === 100) {
    message = 'Zoom reset to 100%.'
  } else if (newSize > oldSize) {
    message = `Zoom increased to ${String(newSize)}%. ${resetHint}`
  } else {
    message = `Zoom decreased to ${String(newSize)}%. ${resetHint}`
  }

  addToast(message, { level: 'info', id: 'zoom-change' })
}

/**
 * Typed dispatch entry point. The generic `K` keeps the public signature
 * arg-checked per command (arg-less ids take no second argument; arg-carrying
 * ones like `view.setMode` require their typed payload). Inside, `commandId`
 * widens back to the `CommandId` union so the `switch` narrows per `case` as
 * before, and the single arg payload is read per-case from `dispatchArgs`.
 */
// eslint-disable-next-line complexity -- Command dispatcher handles many cases; switch is the clearest pattern
export async function handleCommandExecute<K extends CommandId>(
  commandId: K,
  ctx: CommandDispatchContext,
  ...args: CommandDispatchArgs<K>
): Promise<void> {
  // Widen the generic so the switch narrows on the union (a generic `K` doesn't
  // narrow per `case`). The lone arg payload, if any, is read per-case below.
  const id: CommandId = commandId
  const dispatchArgs: CommandArgs[CommandId] | undefined = args[0]
  const explorerRef = ctx.getExplorer()

  // Bail before logging if the user's intent is text manipulation in a selectable
  // region. Native menu accelerators (⌘C, ⌘A) flow through here even when focus is
  // outside the file pane, so without this guard every text copy would log
  // `edit.copy` / `selection.selectAll` and trigger file-scope behavior.
  if (handleTextRegionShortcut(id)) return

  // Every keyboard / palette / menu command flows through here. Two channels:
  // - Info-level structured log → LogTape → Rust bridge → fern file chain, so the
  //   line appears alongside backend logs in error-report bundles.
  // - A `kind: "command"` breadcrumb → the manifest's rolling buffer, so triagers
  //   see what the user did right before an error fired.
  log.info(id)
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- excluded from typed bindings (see ipc/CLAUDE.md); tracked for follow-up when specta supports skip_serializing_if
  void invoke('record_breadcrumb', { kind: 'command', message: id, ctx: null }).catch(() => {
    // Best-effort: a failing breadcrumb shouldn't break the dispatch.
  })

  ctx.dialogs.showCommandPalette(false)

  // Block destination-side actions on a search-results pane with a friendly toast
  // (M8c). Menu paths are visibly disabled at the source; this catches the
  // shortcut-driven path that bypasses the UI.
  if (blockedBySearchResultsPane(id, explorerRef)) return

  // Handle known commands by category
  switch (id) {
    // === App commands ===
    // app.quit, app.hide, app.hideOthers, app.showAll are native-only;
    // handled by PredefinedMenuItems (terminate:, hide:, etc.), not JS dispatch.

    case 'app.commandPalette':
      ctx.dialogs.showCommandPalette(true)
      return

    case 'search.open':
      ctx.dialogs.showSearchDialog(true)
      return

    case 'nav.goToPath':
      ctx.dialogs.showGoToPathDialog(true)
      return

    case 'app.settings':
      void openSettingsWindow()
      return

    case 'app.about':
      ctx.dialogs.showAboutWindow(true)
      return

    case 'app.licenseKey':
      ctx.dialogs.showLicenseKeyDialog(true)
      return

    case 'help.sendErrorReport':
      openErrorReportDialog()
      return

    case 'app.checkForUpdates':
      void runMenuTriggeredCheck()
      return

    case 'cmdr.openOnboarding':
      ctx.dialogs.openOnboarding()
      return

    // === View commands ===
    case 'view.showHidden': {
      // Local-first toggle: flip FE state synchronously so the listing
      // re-fetch effects land in the next Svelte tick, then push the new
      // check state to the native menu fire-and-forget. The previous
      // implementation routed through `toggle_hidden_files` (Rust toggle +
      // `settings-changed` emit + FE listener), which added an IPC + event
      // hop and caused the `toggles hidden file visibility` e2e test to
      // flake ~1/25 runs under slow-lane load.
      if (!explorerRef) return
      const newState = explorerRef.toggleHiddenFiles()
      void syncMenuShowHidden(newState)
      return
    }

    case 'view.briefMode':
      explorerRef?.setViewMode('brief')
      return

    case 'view.fullMode':
      explorerRef?.setViewMode('full')
      return

    case 'view.setMode': {
      // Per-pane view change. The `id === 'view.setMode'` narrowing doesn't reach
      // `dispatchArgs` (it's a separate local), so read the typed payload with a
      // single cast — the generic signature already type-checked it at the call
      // site. `fromMenu` picks the primitive: a native-menu click
      // (`view-mode-changed`, `fromMenu: true`) routes to `setViewModeFromMenu`,
      // which skips `pushViewMenuState` because the click already toggled its own
      // CheckMenuItem (Rust ran `sync_view_mode_check_states`); the MCP
      // `set_view_mode` tool (`fromMenu: false`) routes to `setViewMode`, which
      // pushes the menu state since nothing toggled it.
      const { pane, mode, fromMenu } = dispatchArgs as CommandArgs['view.setMode']
      if (fromMenu) explorerRef?.setViewModeFromMenu(pane, mode)
      else explorerRef?.setViewMode(mode, pane)
      return
    }

    // === Zoom commands ===
    // Each writes `appearance.textSize`; the settings store cross-window-syncs
    // and `lib/text-size.svelte.ts` recomputes the effective scale.
    case 'view.zoom.set75':
    case 'view.zoom.set100':
    case 'view.zoom.set125':
    case 'view.zoom.set150': {
      const preset = {
        'view.zoom.set75': 75,
        'view.zoom.set100': 100,
        'view.zoom.set125': 125,
        'view.zoom.set150': 150,
      }[id]
      const current = getSetting('appearance.textSize')
      setSetting('appearance.textSize', preset)
      showZoomToast(current, preset)
      return
    }
    case 'view.zoom.in': {
      const current = getSetting('appearance.textSize')
      const next = Math.min(150, current + 10)
      setSetting('appearance.textSize', next)
      showZoomToast(current, next)
      return
    }
    case 'view.zoom.out': {
      const current = getSetting('appearance.textSize')
      const next = Math.max(75, current - 10)
      setSetting('appearance.textSize', next)
      showZoomToast(current, next)
      return
    }

    // === Pane commands ===
    case 'pane.switch':
      explorerRef?.switchPane()
      return

    case 'pane.swap':
      explorerRef?.swapPanes()
      return

    case 'pane.leftVolumeChooser':
      explorerRef?.toggleVolumeChooser('left')
      return

    case 'pane.rightVolumeChooser':
      explorerRef?.toggleVolumeChooser('right')
      return

    case 'pane.copyPathLeftToRight':
      explorerRef?.copyPathBetweenPanes('left', 'right')
      return

    case 'pane.copyPathRightToLeft':
      explorerRef?.copyPathBetweenPanes('right', 'left')
      return

    case 'pane.refresh':
      // MCP `refresh` tool: re-list the focused pane.
      explorerRef?.refreshPane()
      return

    // === Tab commands ===
    case 'tab.new': {
      const success = explorerRef?.newTab()
      if (success === false) {
        addToast('Tab limit reached', { level: 'warn' })
      }
      return
    }

    case 'tab.close': {
      const result = await explorerRef?.closeActiveTabWithConfirmation()
      if (result === 'last-tab') {
        const { getCurrentWindow } = await import('@tauri-apps/api/window')
        await getCurrentWindow().close()
      }
      return
    }

    case 'tab.reopen': {
      const result = explorerRef?.reopenLastClosedTab()
      if (result === 'empty') {
        addToast('No recently closed tabs in this pane.', { level: 'warn' })
      } else if (result === 'cap') {
        addToast('Tab limit reached', { level: 'warn' })
      }
      return
    }

    case 'tab.next':
      explorerRef?.cycleTab('next')
      return

    case 'tab.prev':
      explorerRef?.cycleTab('prev')
      return

    case 'tab.togglePin':
      explorerRef?.togglePinActiveTab()
      return

    case 'tab.closeOthers':
      explorerRef?.closeOtherTabs()
      return

    case 'tab.mcpAction': {
      // MCP `tab` tool: a per-pane tab action targeting a SPECIFIC pane and tab
      // (the focused-pane `tab.new`/`tab.close`/etc. can't). Routes to the
      // component's `handleMcpTabAction`, which owns the tab-mutation primitives.
      const { pane, action, tabId, pinned } = dispatchArgs as CommandArgs['tab.mcpAction']
      explorerRef?.handleMcpTabAction(pane, action, tabId, pinned)
      return
    }

    // === Navigation commands ===
    case 'nav.open':
      explorerRef?.sendKeyToFocusedPane('Enter')
      return

    case 'nav.parent':
      explorerRef?.navigate('parent')
      return

    case 'nav.back':
      explorerRef?.navigate('back')
      return

    case 'nav.forward':
      explorerRef?.navigate('forward')
      return

    case 'nav.home':
      explorerRef?.sendKeyToFocusedPane('Home')
      return

    case 'nav.end':
      explorerRef?.sendKeyToFocusedPane('End')
      return

    case 'nav.pageUp':
      explorerRef?.sendKeyToFocusedPane('PageUp')
      return

    case 'nav.pageDown':
      explorerRef?.sendKeyToFocusedPane('PageDown')
      return

    case 'nav.openUnderCursor':
      // MCP `open_under_cursor` round-trip: AWAIT so the adapter's
      // `emit('mcp-response', { ok: true })` fires only after the open completes
      // (directory listed, or OS open-with-default dispatched). An exception
      // propagates to the adapter's try/catch, which replies `ok: false`.
      await explorerRef?.openItemUnderCursor()
      return

    case 'cursor.moveTo': {
      // MCP `move_cursor` round-trip: AWAIT for the same ack-timing reason. L1/L2
      // (focus re-anchor + `whenLoadSettles`) live inside `moveCursor` — untouched.
      const { pane, to } = dispatchArgs as CommandArgs['cursor.moveTo']
      await explorerRef?.moveCursor(pane, to)
      return
    }

    case 'cursor.scrollTo': {
      const { pane, index } = dispatchArgs as CommandArgs['cursor.scrollTo']
      explorerRef?.scrollTo(pane, index)
      return
    }

    // === Downloads commands ===
    case 'downloads.goToLatest':
      await goToLatestDownload(explorerRef)
      return

    // === Network commands ===
    case 'network.refresh':
      explorerRef?.refreshNetworkHosts()
      return

    // === Volume commands ===
    case 'volume.selectByName': {
      // MCP `select_volume` tool: select a SPECIFIC pane's volume by name.
      // Navigation-adjacent — still calls `selectVolumeByName` (Phase 3 owns
      // volume mechanics).
      const { pane, name } = dispatchArgs as CommandArgs['volume.selectByName']
      void explorerRef?.selectVolumeByName(pane, name)
      return
    }

    // === Sort commands ===
    case 'sort.byName':
      explorerRef?.setSortColumn('name')
      return

    case 'sort.byExtension':
      explorerRef?.setSortColumn('extension')
      return

    case 'sort.bySize':
      explorerRef?.setSortColumn('size')
      return

    case 'sort.byModified':
      explorerRef?.setSortColumn('modified')
      return

    case 'sort.byCreated':
      explorerRef?.setSortColumn('created')
      return

    case 'sort.ascending':
      explorerRef?.setSortOrder('asc')
      return

    case 'sort.descending':
      explorerRef?.setSortOrder('desc')
      return

    case 'sort.toggleOrder':
      explorerRef?.setSortOrder('toggle')
      return

    case 'sort.set': {
      // Per-pane sort from the MCP `sort` tool: an explicit column + order on a
      // SPECIFIC pane (the `sort.by*` / `sort.ascending` commands act on the
      // focused pane only).
      const { pane, column, order } = dispatchArgs as CommandArgs['sort.set']
      void explorerRef?.setSort(column, order, pane)
      return
    }

    // === File action commands ===
    case 'file.view':
      void explorerRef?.openViewerForCursor()
      return

    case 'file.rename':
      explorerRef?.startRename()
      return

    case 'file.edit': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        await openInEditor(entryUnderCursor.path)
      }
      return
    }

    case 'file.copy': {
      // Arg-less from the F-bar / palette / keyboard (open the dialog with no
      // preset); the MCP `copy` tool may pass `{ autoConfirm, onConflict }` to
      // pre-answer the conflict policy. `dispatchArgs` is `undefined` in the
      // arg-less case, so the openers default both.
      const copyArgs = dispatchArgs as CommandArgs['file.copy'] | undefined
      void explorerRef?.openCopyDialog(copyArgs?.autoConfirm, copyArgs?.onConflict)
      return
    }

    case 'file.move': {
      const moveArgs = dispatchArgs as CommandArgs['file.move'] | undefined
      void explorerRef?.openMoveDialog(moveArgs?.autoConfirm, moveArgs?.onConflict)
      return
    }

    case 'file.newFolder':
      void explorerRef?.openNewFolderDialog()
      return

    case 'file.newFile':
      void explorerRef?.openNewFileDialog()
      return

    case 'file.delete': {
      const deleteArgs = dispatchArgs as CommandArgs['file.delete'] | undefined
      void explorerRef?.openDeleteDialog(false, deleteArgs?.autoConfirm)
      return
    }

    case 'file.deletePermanently':
      void explorerRef?.openDeleteDialog(true)
      return

    case 'dialog.confirm': {
      // MCP `dialog confirm` tool: programmatically confirm an already-open
      // transfer/delete dialog.
      const { type, onConflict } = dispatchArgs as CommandArgs['dialog.confirm']
      explorerRef?.confirmDialog(type, onConflict)
      return
    }

    case 'file.showInFinder': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        await showInFinder(entryUnderCursor.path)
      }
      return
    }

    case 'file.copyPath': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        await copyToClipboard(entryUnderCursor.path)
      }
      return
    }

    case 'file.copyCurrentDirectoryPath': {
      const currentPath = getFocusedPanePath()
      if (currentPath) {
        await copyToClipboard(currentPath)
      }
      return
    }

    case 'file.copyFilename': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        await copyToClipboard(entryUnderCursor.filename)
      }
      return
    }

    case 'file.quickLook': {
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
      const volumeId = getFocusedPaneVolumeId()
      // Optimistically flip `isOpen` before the IPC: AppKit returns from
      // `makeKeyAndOrderFront:` synchronously and the panel is up by the time
      // the IPC resolves, but the optimistic flip means a second Shift+Space
      // press immediately after the first reads the right state.
      quickLookState.isOpen = true
      await quickLookOpen(entryUnderCursor.path, volumeId)
      return
    }

    case 'file.getInfo': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        await getInfo(entryUnderCursor.path)
      }
      return
    }

    case 'cloud.makeOffline': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        try {
          await cloudMakeAvailableOffline(entryUnderCursor.path)
        } catch (e) {
          addToast(`Couldn't download from cloud. ${String(e)}`, { level: 'error' })
        }
      }
      return
    }

    case 'cloud.removeDownload': {
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        try {
          await cloudRemoveDownload(entryUnderCursor.path)
        } catch (e) {
          addToast(`Couldn't remove the download. ${String(e)}`, { level: 'error' })
        }
      }
      return
    }

    // === Selection commands ===
    case 'selection.toggle':
      explorerRef?.handleSelectionAction('toggleAtCursor')
      return

    case 'selection.toggleAndDown':
      explorerRef?.handleSelectionAction('toggleAtCursorAndMoveDown')
      return

    case 'selection.selectAll': {
      // ⌘A is a native menu accelerator (so it shows in the Edit menu), which means
      // macOS intercepts it before the webview. When a text input is focused, route
      // to the input's select-all instead of file selection.
      const active = document.activeElement
      if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) {
        active.select()
        return
      }
      explorerRef?.handleSelectionAction('selectAll')
      return
    }

    case 'selection.deselectAll':
      explorerRef?.handleSelectionAction('deselectAll')
      return

    case 'selection.mcpSelect': {
      // MCP `select` tool: range/all selection on a SPECIFIC pane with a typed
      // mode (`replace`/`add`/`subtract`).
      const { pane, start, count, mode } = dispatchArgs as CommandArgs['selection.mcpSelect']
      explorerRef?.handleMcpSelect(pane, start, count, mode)
      return
    }

    case 'selection.selectFiles':
      ctx.dialogs.showSelectionDialog('add')
      return

    case 'selection.deselectFiles':
      ctx.dialogs.showSelectionDialog('remove')
      return

    // === Edit commands (clipboard) ===
    case 'edit.copy': {
      const active = document.activeElement
      if (
        active instanceof HTMLInputElement ||
        active instanceof HTMLTextAreaElement ||
        active?.closest('[contenteditable]')
      ) {
        // eslint-disable-next-line @typescript-eslint/no-deprecated -- No modern alternative for triggering native copy in text inputs
        document.execCommand('copy')
        return
      }
      // If the user has selected text anywhere with `user-select: text`
      // (for example, the ErrorPane), prefer copying that text over the file
      // selection. Note: the +page.svelte global keydown bail doesn't help on
      // macOS, where the native Edit > Copy menu accelerator fires before JS
      // sees the keydown; this branch is the actual entry point in that case.
      const selection = window.getSelection()
      if (selection && !selection.isCollapsed && selection.toString().length > 0) {
        void navigator.clipboard.writeText(selection.toString())
        return
      }
      void explorerRef?.copyToClipboard()
      return
    }

    case 'edit.cut': {
      const active = document.activeElement
      if (
        active instanceof HTMLInputElement ||
        active instanceof HTMLTextAreaElement ||
        active?.closest('[contenteditable]')
      ) {
        // eslint-disable-next-line @typescript-eslint/no-deprecated -- No modern alternative for triggering native cut in text inputs
        document.execCommand('cut')
        return
      }
      void explorerRef?.cutToClipboard()
      return
    }

    case 'edit.paste': {
      const active = document.activeElement
      if (
        active instanceof HTMLInputElement ||
        active instanceof HTMLTextAreaElement ||
        active?.closest('[contenteditable]')
      ) {
        // Read clipboard text via Rust (bypasses WebKit's navigator.clipboard
        // permission popup that shows a "Paste" button the user must click).
        const text = await readClipboardText()
        if (text) {
          // eslint-disable-next-line @typescript-eslint/no-deprecated -- insertText is the only way to insert at cursor position in inputs
          document.execCommand('insertText', false, text)
        }
        return
      }
      void explorerRef?.pasteFromClipboard(false)
      return
    }

    case 'edit.pasteAsMove':
      // Option+Cmd+V is not a text shortcut, so no activeElement check needed
      void explorerRef?.pasteFromClipboard(true)
      return

    // === About window commands ===
    case 'about.openWebsite':
      await openExternalUrl('https://getcmdr.com')
      return

    case 'about.openUpgrade':
      await openExternalUrl('https://getcmdr.com/upgrade')
      return

    case 'about.close':
      ctx.dialogs.showAboutWindow(false)
      return
  }
}
