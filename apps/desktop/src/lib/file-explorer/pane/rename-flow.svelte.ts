import { getFileRange } from '$lib/tauri-commands'
import { moveToTrash, type RenameValidityResult } from '$lib/tauri-commands'
import { validateFilename, getExtension } from '$lib/utils/filename-validation'
import { cancelClickToRename } from '../rename/rename-activation'
import { executeRenameSave, performRename, checkPermission, type RenameResult } from '../rename/rename-operations'
import { getSetting } from '$lib/settings'
import type { ConflictResolution } from '../rename/RenameConflictDialog.svelte'
import { addToast, dismissTransientToasts } from '$lib/ui/toast'
import type { FileEntry } from '../types'
import type { createRenameState } from '../rename/rename-state.svelte'

export interface RenameFlowDeps {
    rename: ReturnType<typeof createRenameState>
    getListingId: () => string
    getTotalCount: () => number
    getIncludeHidden: () => boolean
    getCurrentPath: () => string
    getCursorIndex: () => number
    getShowHiddenFiles: () => boolean
    getEntryUnderCursor: () => FileEntry | undefined
    onRequestFocus: () => void
}

export function createRenameFlow(deps: RenameFlowDeps) {
    const { rename, onRequestFocus } = deps

    // Extension change dialog state
    let extensionDialogState = $state<{ oldExtension: string; newExtension: string } | null>(null)

    // Conflict dialog state
    let conflictDialogState = $state<{
        validity: RenameValidityResult
        trimmedName: string
    } | null>(null)

    // Post-rename: name to select after file watcher refresh
    let pendingCursorName = $state<string | null>(null)

    // Sibling names cache for rename conflict detection (loaded once when rename starts)
    let renameSiblingNames: string[] = []

    // When true, suppress the blur-cancel (a dialog is about to open)
    let suppressBlurCancel = false

    async function loadSiblingNames(excludeName: string): Promise<string[]> {
        const listingId = deps.getListingId()
        const totalCount = deps.getTotalCount()
        const includeHidden = deps.getIncludeHidden()
        if (!listingId || totalCount === 0) return []
        try {
            const batchSize = 500
            const names: string[] = []
            for (let start = 0; start < totalCount; start += batchSize) {
                const count = Math.min(batchSize, totalCount - start)
                const entries = await getFileRange(listingId, start, count, includeHidden)
                for (const entry of entries) {
                    if (entry.name !== excludeName) {
                        names.push(entry.name)
                    }
                }
            }
            return names
        } catch {
            return []
        }
    }

    function handleRenameResult(result: RenameResult, trimmedName: string) {
        switch (result.type) {
            case 'noop':
                rename.cancel()
                onRequestFocus()
                break
            case 'error':
                rename.triggerShake()
                addToast(result.message, { level: 'error' })
                break
            case 'extension-ask':
                suppressBlurCancel = true
                extensionDialogState = {
                    oldExtension: result.oldExtension,
                    newExtension: result.newExtension,
                }
                break
            case 'conflict':
                suppressBlurCancel = true
                conflictDialogState = { validity: result.validity, trimmedName }
                break
            case 'success':
                finalizeRename(result.newName)
                break
        }
    }

    function finalizeRename(newName: string) {
        const wasHiddenRename = newName.startsWith('.') && !deps.getShowHiddenFiles()

        rename.cancel()
        extensionDialogState = null
        conflictDialogState = null
        onRequestFocus()

        pendingCursorName = newName

        if (wasHiddenRename) {
            addToast("Your file disappeared from view because hidden files aren't shown.")
        }
    }

    async function executeFlow(skipExtensionCheck?: boolean) {
        const target = rename.target
        if (!target) return

        const trimmedName = rename.getTrimmedName()
        const extensionPolicy = getSetting('fileOperations.allowFileExtensionChanges')

        const result = await executeRenameSave(target, trimmedName, extensionPolicy, skipExtensionCheck)
        handleRenameResult(result, trimmedName)
    }

    return {
        get extensionDialogState() {
            return extensionDialogState
        },
        get conflictDialogState() {
            return conflictDialogState
        },
        get pendingCursorName() {
            return pendingCursorName
        },
        set pendingCursorName(v: string | null) {
            pendingCursorName = v
        },

        startRename(): void {
            const entry = deps.getEntryUnderCursor()
            if (!entry || entry.name === '..') return

            const target = {
                path: entry.path,
                originalName: entry.name,
                parentPath: deps.getCurrentPath(),
                index: deps.getCursorIndex(),
                isDirectory: entry.isDirectory,
            }

            rename.activate(target)
            renameSiblingNames = []

            void loadSiblingNames(entry.name).then((names) => {
                renameSiblingNames = names
            })

            void checkPermission(entry.path).then((errorMsg) => {
                if (errorMsg && rename.active && rename.target?.path === entry.path) {
                    rename.cancel()
                    addToast(errorMsg, { level: 'error' })
                    onRequestFocus()
                }
            })
        },

        cancelRename(): void {
            cancelClickToRename()
            rename.cancel()
            renameSiblingNames = []
            extensionDialogState = null
            conflictDialogState = null
            onRequestFocus()
        },

        handleRenameInput(value: string) {
            rename.setCurrentName(value)
            dismissTransientToasts()
            const extensionPolicy = getSetting('fileOperations.allowFileExtensionChanges')
            const result = validateFilename(
                value,
                rename.target?.originalName ?? '',
                deps.getCurrentPath(),
                renameSiblingNames,
                extensionPolicy,
            )
            rename.setValidation(result)
        },

        handleRenameSubmit() {
            if (rename.severity === 'error') {
                rename.triggerShake()
                addToast(rename.validation.message, { level: 'error' })
                return
            }
            if (!rename.hasChanged()) {
                rename.cancel()
                onRequestFocus()
                return
            }
            void executeFlow()
        },

        handleExtensionKeepOld() {
            extensionDialogState = null
            if (rename.target) {
                const oldExt = getExtension(rename.target.originalName)
                const nameWithoutExt = rename.getTrimmedName()
                const newExt = getExtension(nameWithoutExt)
                if (newExt) {
                    const base = nameWithoutExt.slice(0, -newExt.length)
                    rename.setCurrentName(base + oldExt)
                }
            }
            rename.requestRefocus()
        },

        handleExtensionUseNew() {
            extensionDialogState = null
            void executeFlow(true)
        },

        handleConflictResolve(resolution: ConflictResolution) {
            const target = rename.target
            const trimmedName = conflictDialogState?.trimmedName
            conflictDialogState = null

            if (!target || !trimmedName) {
                rename.cancel()
                onRequestFocus()
                return
            }

            switch (resolution) {
                case 'overwrite-trash': {
                    const conflictPath = target.parentPath + '/' + trimmedName
                    void moveToTrash(conflictPath)
                        .then(() => performRename(target, trimmedName, true))
                        .then((result) => {
                            handleRenameResult(result, trimmedName)
                        })
                        .catch((e: unknown) => {
                            const msg = e instanceof Error ? e.message : String(e)
                            addToast(msg, { level: 'error' })
                            rename.cancel()
                            onRequestFocus()
                        })
                    break
                }
                case 'overwrite-delete':
                    void performRename(target, trimmedName, true).then((result) => {
                        handleRenameResult(result, trimmedName)
                    })
                    break
                case 'cancel':
                    rename.cancel()
                    onRequestFocus()
                    break
                case 'continue':
                    rename.requestRefocus()
                    break
            }
        },

        handleRenameCancel() {
            if (suppressBlurCancel) {
                suppressBlurCancel = false
                return
            }
            rename.cancel()
            onRequestFocus()
        },

        handleRenameShakeEnd() {
            rename.clearShake()
        },
    }
}
