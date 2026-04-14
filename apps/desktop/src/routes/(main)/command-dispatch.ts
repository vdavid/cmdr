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
    toggleHiddenFiles,
    setViewMode,
    readClipboardText,
} from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { openSettingsWindow } from '$lib/settings/settings-window'
import type { ExplorerAPI } from './explorer-api'

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

// eslint-disable-next-line complexity -- Command dispatcher handles many cases; switch is the clearest pattern
export async function handleCommandExecute(commandId: string, ctx: CommandDispatchContext): Promise<void> {
    const explorerRef = ctx.getExplorer()

    ctx.dialogs.showCommandPalette(false)

    // Handle known commands by category
    switch (commandId) {
        // === App commands ===
        // app.quit, app.hide, app.hideOthers, app.showAll are native-only —
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

        // === View commands ===
        case 'view.showHidden':
            // Use Tauri command to toggle and sync menu checkbox state
            await toggleHiddenFiles()
            return

        case 'view.briefMode':
            // Use Tauri command to set mode and sync menu radio state
            await setViewMode('brief')
            return

        case 'view.fullMode':
            // Use Tauri command to set mode and sync menu radio state
            await setViewMode('full')
            return

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
