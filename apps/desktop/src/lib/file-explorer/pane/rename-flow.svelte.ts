import { getFileRange, refreshListing } from '$lib/tauri-commands'
import { getIpcErrorMessage, isIpcError, moveToTrash, type RenameValidityResult } from '$lib/tauri-commands'
import { validateFilename, getExtension } from '$lib/utils/filename-validation'
import { cancelClickToRename } from '../rename/rename-activation'
import { executeRenameSave, performRename, checkPermission, type RenameResult } from '../rename/rename-operations'
import { getSetting } from '$lib/settings'
import type { RenameConflictResolution } from '../rename/rename-operations'
import { addToastForPane, dismissTransientToastsForPane, type ToastOriginPane } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { pathInsideArchive } from './volume-capabilities'
import type { FileEntry } from '../types'
import type { StartRenameOptions } from './types'
import type { createRenameState } from '../rename/rename-state.svelte'

export interface RenameFlowDeps {
  rename: ReturnType<typeof createRenameState>
  /** Owning pane, so rename feedback and per-keystroke dismissal stay pane-scoped. */
  paneId: ToastOriginPane
  getListingId: () => string
  getTotalCount: () => number
  getIncludeHidden: () => boolean
  getCurrentPath: () => string
  getShowHiddenFiles: () => boolean
  getVolumeId: () => string
  getEntryUnderCursor: () => FileEntry | undefined
  onRequestFocus: () => void
}

export function createRenameFlow(deps: RenameFlowDeps) {
  const { rename, onRequestFocus } = deps

  // Rename feedback is pane-local: tag it so only this pane's navigation clears it.
  const addToast = (content: Parameters<typeof addToastForPane>[1], options?: Parameters<typeof addToastForPane>[2]) =>
    addToastForPane(deps.paneId, content, options)

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

  // When true, treat the extension-change policy as 'yes' for the current rename
  // session (no warning/dialog). Set by an auto-started rename (paste-clipboard-
  // as-file) and reset when the session ends. F2/user renames leave it false.
  let suppressExtensionWarningOnce = false

  /** The extension-change policy in effect for this rename session. */
  function effectiveExtensionPolicy() {
    return suppressExtensionWarningOnce ? 'yes' : getSetting('fileOperations.allowFileExtensionChanges')
  }

  // Auto-rename activation (paste-clipboard-as-file): the created file's row may
  // not be under the cursor yet when startRename runs (the optimistic cursor move
  // can beat the synthetic directory-diff), so we NEVER activate on a mismatched
  // entry — we poll until the expected file is under the cursor, then activate,
  // and give up silently after a bounded window (file kept, no rename). This makes
  // "rename grabs the wrong file" impossible by construction, not by timing.
  let pendingRenameActivation: ReturnType<typeof setInterval> | null = null
  const RENAME_ACTIVATION_POLL_MS = 50
  const RENAME_ACTIVATION_TIMEOUT_MS = 2000

  function clearPendingRenameActivation() {
    if (pendingRenameActivation !== null) {
      clearInterval(pendingRenameActivation)
      pendingRenameActivation = null
    }
  }

  /** Activates the inline rename editor on `entry` (the real activation body). */
  function activateRename(entry: FileEntry): void {
    const target = {
      path: entry.path,
      originalName: entry.name,
      parentPath: deps.getCurrentPath(),
      isDirectory: entry.isDirectory,
    }

    rename.activate(target)
    renameSiblingNames = []

    void loadSiblingNames(entry.name).then((names) => {
      renameSiblingNames = names
    })

    // Skip the permission check for MTP AND archive-inner paths (see startRename below).
    const currentVolumeId = deps.getVolumeId()
    if (!currentVolumeId.startsWith('mtp-') && !pathInsideArchive(entry.path)) {
      void checkPermission(entry.path).then((errorMsg) => {
        if (errorMsg && rename.active && rename.target?.path === entry.path) {
          rename.cancel()
          addToast(errorMsg, { level: 'error' })
          onRequestFocus()
        }
      })
    }
  }

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
      case 'timeout':
        rename.cancel()
        onRequestFocus()
        addToast(result.message, { level: 'warn', dismissal: 'persistent' })
        void refreshListing(deps.getListingId())
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

    clearPendingRenameActivation()
    rename.cancel()
    extensionDialogState = null
    conflictDialogState = null
    suppressExtensionWarningOnce = false
    onRequestFocus()

    pendingCursorName = newName

    if (wasHiddenRename) {
      addToast(tString('fileExplorer.rename.hiddenAfterRename'), { level: 'info' })
    }
  }

  async function executeFlow(skipExtensionCheck?: boolean) {
    const target = rename.target
    if (!target) return

    const trimmedName = rename.getTrimmedName()
    const extensionPolicy = effectiveExtensionPolicy()
    const currentVolumeId = deps.getVolumeId()

    const result = await executeRenameSave(target, trimmedName, extensionPolicy, skipExtensionCheck, currentVolumeId)
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

    startRename(options?: StartRenameOptions): void {
      // A fresh startRename supersedes any pending auto-activation poll.
      clearPendingRenameActivation()

      // Scoped to this rename session; reset when it ends (finalize/cancel).
      suppressExtensionWarningOnce = options?.suppressExtensionWarning ?? false
      const expectedName = options?.expectedName

      // Activate ONLY on the intended entry. The permission check (skipped for MTP
      // and archive-inner paths) and sibling-name load live in `activateRename`.
      // `expectedName` (auto-started rename) guards against latching a DIFFERENT
      // file when the cursor move beats the new file's synthetic diff — a
      // data-safety hazard, since the next keystroke would rename that other file.
      const tryActivate = (): boolean => {
        const entry = deps.getEntryUnderCursor()
        if (!entry || entry.name === '..') return false
        if (expectedName !== undefined && entry.name !== expectedName) return false
        activateRename(entry)
        return true
      }

      if (tryActivate()) return

      // No expectedName = user-initiated rename (F2) with no valid entry under the
      // cursor → bail (matches the prior no-entry behavior).
      if (expectedName === undefined) return

      // Auto-rename whose target row hasn't landed under the cursor yet: poll until
      // it does, then activate; give up silently after the bounded window.
      let elapsed = 0
      pendingRenameActivation = setInterval(() => {
        elapsed += RENAME_ACTIVATION_POLL_MS
        if (tryActivate() || elapsed >= RENAME_ACTIVATION_TIMEOUT_MS) {
          clearPendingRenameActivation()
        }
      }, RENAME_ACTIVATION_POLL_MS)
    },

    cancelRename(): void {
      clearPendingRenameActivation()
      cancelClickToRename()
      rename.cancel()
      renameSiblingNames = []
      extensionDialogState = null
      conflictDialogState = null
      suppressExtensionWarningOnce = false
      onRequestFocus()
    },

    handleRenameInput(value: string) {
      rename.setCurrentName(value)
      dismissTransientToastsForPane(deps.paneId)
      const extensionPolicy = effectiveExtensionPolicy()
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

    handleConflictResolve(resolution: RenameConflictResolution) {
      const target = rename.target
      const trimmedName = conflictDialogState?.trimmedName
      conflictDialogState = null

      if (!target || !trimmedName) {
        rename.cancel()
        onRequestFocus()
        return
      }

      const currentVolumeId = deps.getVolumeId()

      switch (resolution) {
        case 'overwrite-trash': {
          const conflictPath = target.parentPath + '/' + trimmedName
          void moveToTrash(conflictPath)
            .then(() => performRename(target, trimmedName, true, currentVolumeId))
            .then((result) => {
              handleRenameResult(result, trimmedName)
            })
            .catch((e: unknown) => {
              if (isIpcError(e) && e.timedOut) {
                addToast(tString('fileExplorer.pane.trashUnconfirmedToast'), {
                  level: 'warn',
                  dismissal: 'persistent',
                })
                void refreshListing(deps.getListingId())
              } else {
                addToast(getIpcErrorMessage(e), { level: 'error' })
              }
              rename.cancel()
              onRequestFocus()
            })
          break
        }
        case 'overwrite-delete':
          void performRename(target, trimmedName, true, currentVolumeId).then((result) => {
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
