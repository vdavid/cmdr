/**
 * Command dispatch: maps command IDs from the command palette, keyboard shortcuts,
 * and menu actions to concrete app actions.
 */

import {
  openExternalUrl,
  showInFinder,
  copyToClipboard,
  quickLook,
  getInfo,
  openInEditor,
  syncMenuShowHidden,
  readClipboardText,
  cloudMakeAvailableOffline,
  cloudRemoveDownload,
} from '$lib/tauri-commands'
import { invoke } from '@tauri-apps/api/core'
import { addToast } from '$lib/ui/toast'
import { getSetting, setSetting } from '$lib/settings'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
import { runMenuTriggeredCheck } from '$lib/updates/updater.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { ExplorerAPI } from './explorer-api'

const log = getAppLogger('user-action')

/** Callbacks for toggling dialog visibility from command dispatch */
export interface CommandDispatchDialogs {
  showCommandPalette: (show: boolean) => void
  showSearchDialog: (show: boolean) => void
  showAboutWindow: (show: boolean) => void
  showLicenseKeyDialog: (show: boolean) => void
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
 * Intercepts text-region shortcuts (⌘C, ⌘A) BEFORE the dispatcher logs or records
 * the action, so selecting text in the ErrorPane and copying it doesn't pollute the
 * user-action log used for rollback context, and doesn't fire file-scope side
 * effects (copy files, select all files). Returns `true` if the shortcut was handled.
 *
 * For `edit.copy` we only intercept when the selection is non-collapsed (something is
 * actually selected); otherwise we fall through so the file copy path can run.
 */
function handleTextRegionShortcut(commandId: string): boolean {
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

// eslint-disable-next-line complexity -- Command dispatcher handles many cases; switch is the clearest pattern
export async function handleCommandExecute(commandId: string, ctx: CommandDispatchContext): Promise<void> {
  const explorerRef = ctx.getExplorer()

  // Bail before logging if the user's intent is text manipulation in a selectable
  // region. Native menu accelerators (⌘C, ⌘A) flow through here even when focus is
  // outside the file pane, so without this guard every text copy would log
  // `edit.copy` / `selection.selectAll` and trigger file-scope behavior.
  if (handleTextRegionShortcut(commandId)) return

  // Every keyboard / palette / menu command flows through here. Two channels:
  // - Info-level structured log → LogTape → Rust bridge → fern file chain, so the
  //   line appears alongside backend logs in error-report bundles.
  // - A `kind: "command"` breadcrumb → the manifest's rolling buffer, so triagers
  //   see what the user did right before an error fired.
  log.info(commandId)
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- excluded from typed bindings (see ipc/CLAUDE.md); tracked for follow-up when specta supports skip_serializing_if
  void invoke('record_breadcrumb', { kind: 'command', message: commandId, ctx: null }).catch(() => {
    // Best-effort: a failing breadcrumb shouldn't break the dispatch.
  })

  ctx.dialogs.showCommandPalette(false)

  // Handle known commands by category
  switch (commandId) {
    // === App commands ===
    // app.quit, app.hide, app.hideOthers, app.showAll are native-only;
    // handled by PredefinedMenuItems (terminate:, hide:, etc.), not JS dispatch.

    case 'app.commandPalette':
      ctx.dialogs.showCommandPalette(true)
      return

    case 'search.open':
      ctx.dialogs.showSearchDialog(true)
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

    // === Zoom commands ===
    // Each writes `appearance.textSize`; the settings store cross-window-syncs
    // and `lib/text-size.svelte.ts` recomputes the effective scale.
    case 'view.zoom.set75':
      setSetting('appearance.textSize', 75)
      return
    case 'view.zoom.set100':
      setSetting('appearance.textSize', 100)
      return
    case 'view.zoom.set125':
      setSetting('appearance.textSize', 125)
      return
    case 'view.zoom.set150':
      setSetting('appearance.textSize', 150)
      return
    case 'view.zoom.in': {
      const current = getSetting('appearance.textSize')
      setSetting('appearance.textSize', Math.min(150, current + 10))
      return
    }
    case 'view.zoom.out': {
      const current = getSetting('appearance.textSize')
      setSetting('appearance.textSize', Math.max(75, current - 10))
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

    // === Tab commands ===
    case 'tab.new': {
      const success = explorerRef?.newTab()
      if (success === false) {
        addToast('Tab limit reached')
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
        addToast('No recently closed tabs in this pane.')
      } else if (result === 'cap') {
        addToast('Tab limit reached')
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

    // === Network commands ===
    case 'network.refresh':
      explorerRef?.refreshNetworkHosts()
      return

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

    case 'file.copy':
      void explorerRef?.openCopyDialog()
      return

    case 'file.move':
      void explorerRef?.openMoveDialog()
      return

    case 'file.newFolder':
      void explorerRef?.openNewFolderDialog()
      return

    case 'file.newFile':
      void explorerRef?.openNewFileDialog()
      return

    case 'file.delete':
      void explorerRef?.openDeleteDialog(false)
      return

    case 'file.deletePermanently':
      void explorerRef?.openDeleteDialog(true)
      return

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
      const currentPath = explorerRef?.getFocusedPanePath()
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
      const entryUnderCursor = explorerRef?.getFileAndPathUnderCursor()
      if (entryUnderCursor) {
        await quickLook(entryUnderCursor.path)
      }
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
