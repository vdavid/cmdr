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
import type { createRenameState, RenameSessionId } from '../rename/rename-state.svelte'

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

  // A save already sent to the backend. While one is in flight the session belongs
  // to it: the blur that follows a click-away must not cancel out from under it,
  // and `handleRenameResult` still needs `rename.target` to open a dialog.
  let pendingCommit: Promise<void> | null = null

  // True while a click OUTSIDE the editor drives the flow. That click already
  // decides where focus goes (another row, the other pane, the breadcrumb), so
  // the flow must not yank focus back to this pane when it finishes.
  let commitFromClickAway = false

  /** Hands focus back to the pane, unless a click already claimed it. */
  function restoreFocus() {
    if (commitFromClickAway) return
    onRequestFocus()
  }

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
  function activateRename(entry: FileEntry, initialName?: string): void {
    const target = {
      path: entry.path,
      originalName: entry.name,
      parentPath: deps.getCurrentPath(),
      isDirectory: entry.isDirectory,
    }

    rename.activate(target)
    const sessionId = rename.sessionId
    // Seed a proposed name (MCP `rename`) instead of the current one, and validate
    // it so a bad proposal shows the red border immediately (siblings load async;
    // they re-validate on the first keystroke). The editor still selects the
    // name-minus-extension on mount, so the user can accept it with one keypress.
    if (initialName !== undefined) {
      rename.setCurrentName(initialName)
      const result = validateFilename(
        initialName,
        entry.name,
        deps.getCurrentPath(),
        renameSiblingNames,
        effectiveExtensionPolicy(),
      )
      rename.setValidation(result)
    }
    renameSiblingNames = []

    void loadSiblingNames(entry.name).then((names) => {
      // A load that finishes after a newer session started would hand that
      // session the wrong directory to check its name against.
      if (rename.isSuperseded(sessionId)) return
      renameSiblingNames = names
    })

    // Skip the permission check for MTP AND archive-inner paths (see startRename below).
    const currentVolumeId = deps.getVolumeId()
    if (!currentVolumeId.startsWith('mtp-') && !pathInsideArchive(entry.path)) {
      void checkPermission(entry.path).then((errorMsg) => {
        if (errorMsg && rename.active && !rename.isSuperseded(sessionId)) {
          rename.cancel()
          addToast(errorMsg, { level: 'error' })
          restoreFocus()
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

  /** Says so when the name a rename landed on hides the file from this listing. */
  function toastIfHiddenAfterRename(newName: string) {
    if (newName.startsWith('.') && !deps.getShowHiddenFiles()) {
      addToast(tString('fileExplorer.rename.hiddenAfterRename'), { level: 'info' })
    }
  }

  /**
   * Reports a save whose session has since been superseded by a newer one.
   *
   * Such a save may only SPEAK (a toast, a background refresh); everything it
   * used to steer now belongs to a different file. Cancelling would close the
   * editor the user is typing in, shaking would blame the wrong file, moving the
   * cursor would yank it off the file being edited, and a dialog would ask about
   * a file the user has already moved past. The forbidden moves aren't guarded
   * here, they're absent.
   */
  function reportSupersededResult(result: RenameResult) {
    switch (result.type) {
      case 'success':
        toastIfHiddenAfterRename(result.newName)
        break
      case 'error':
        addToast(result.message, { level: 'error' })
        break
      case 'timeout':
        addToast(result.message, { level: 'warn', dismissal: 'persistent' })
        void refreshListing(deps.getListingId())
        break
      case 'noop':
      case 'conflict':
      case 'extension-ask':
        // Nothing happened on disk, and neither question can be put to a user who
        // has moved on: the edit is dropped.
        break
    }
  }

  function handleRenameResult(result: RenameResult, trimmedName: string, sessionId: RenameSessionId) {
    if (rename.isSuperseded(sessionId)) {
      reportSupersededResult(result)
      return
    }
    switch (result.type) {
      case 'noop':
        rename.cancel()
        restoreFocus()
        break
      case 'error':
        addToast(result.message, { level: 'error' })
        // After a click-away the editor is already blurred, so there's nothing to
        // shake and no focused field to fix the name in: end the session instead
        // of stranding an input the user has to hunt back to.
        if (commitFromClickAway) rename.cancel()
        else rename.triggerShake()
        break
      case 'timeout':
        rename.cancel()
        restoreFocus()
        addToast(result.message, { level: 'warn', dismissal: 'persistent' })
        void refreshListing(deps.getListingId())
        break
      case 'extension-ask':
        // The dialog steals focus and blurs the editor; that blur must not cancel.
        // A click-away already spent the blur, so arming it would eat the NEXT
        // cancel (the user's Escape) instead.
        suppressBlurCancel = !commitFromClickAway
        extensionDialogState = {
          oldExtension: result.oldExtension,
          newExtension: result.newExtension,
        }
        break
      case 'conflict':
        suppressBlurCancel = !commitFromClickAway
        conflictDialogState = { validity: result.validity, trimmedName }
        break
      case 'success':
        finalizeRename(result.newName)
        break
    }
  }

  function finalizeRename(newName: string) {
    clearPendingRenameActivation()
    rename.cancel()
    extensionDialogState = null
    conflictDialogState = null
    suppressExtensionWarningOnce = false
    restoreFocus()

    pendingCursorName = newName

    toastIfHiddenAfterRename(newName)
  }

  async function executeFlow(skipExtensionCheck?: boolean) {
    const target = rename.target
    if (!target) return

    // Captured up front, together with the target: the save answers for the
    // session that sent it, whatever is on screen when it comes back.
    const sessionId = rename.sessionId
    const trimmedName = rename.getTrimmedName()
    const extensionPolicy = effectiveExtensionPolicy()
    const currentVolumeId = deps.getVolumeId()

    const result = await executeRenameSave(target, trimmedName, extensionPolicy, skipExtensionCheck, currentVolumeId)
    handleRenameResult(result, trimmedName, sessionId)
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
      const initialName = options?.initialName

      // Activate ONLY on the intended entry. The permission check (skipped for MTP
      // and archive-inner paths) and sibling-name load live in `activateRename`.
      // `expectedName` (auto-started rename) guards against latching a DIFFERENT
      // file when the cursor move beats the new file's synthetic diff — a
      // data-safety hazard, since the next keystroke would rename that other file.
      const tryActivate = (): boolean => {
        const entry = deps.getEntryUnderCursor()
        if (!entry || entry.name === '..') return false
        if (expectedName !== undefined && entry.name !== expectedName) return false
        activateRename(entry, initialName)
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
      restoreFocus()
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
        restoreFocus()
        return
      }
      void executeFlow()
    },

    /**
     * The user clicked somewhere outside the editor: save, the way Finder does.
     *
     * Keyed off a real click, NEVER off blur — the editor also blurs when its row
     * scrolls out of the virtual window, and renaming a file because the list
     * scrolled would be a nasty surprise. That path still discards.
     */
    handleRenameClickAway() {
      if (!rename.active || pendingCommit) return
      // The editor stays mounted under the extension/conflict dialogs, so clicking
      // a dialog button reaches here too. The dialog owns the decision.
      if (extensionDialogState || conflictDialogState) return

      if (rename.severity === 'error') {
        // Don't trap the click and don't swallow the reason: drop the edit and say
        // why the name didn't stick.
        const reason = rename.validation.message
        rename.cancel()
        addToast(tString('fileExplorer.rename.keptOriginalName', { reason }), { level: 'warn' })
        return
      }
      if (!rename.hasChanged()) {
        rename.cancel()
        return
      }

      commitFromClickAway = true
      pendingCommit = executeFlow().finally(() => {
        pendingCommit = null
        commitFromClickAway = false
      })
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
      const sessionId = rename.sessionId
      const trimmedName = conflictDialogState?.trimmedName
      conflictDialogState = null

      if (!target || !trimmedName) {
        rename.cancel()
        restoreFocus()
        return
      }

      const currentVolumeId = deps.getVolumeId()

      switch (resolution) {
        case 'overwrite-trash': {
          const conflictPath = target.parentPath + '/' + trimmedName
          void moveToTrash(conflictPath)
            .then(() => performRename(target, trimmedName, true, currentVolumeId))
            .then((result) => {
              handleRenameResult(result, trimmedName, sessionId)
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
              if (rename.isSuperseded(sessionId)) return
              rename.cancel()
              restoreFocus()
            })
          break
        }
        case 'overwrite-delete':
          void performRename(target, trimmedName, true, currentVolumeId).then((result) => {
            handleRenameResult(result, trimmedName, sessionId)
          })
          break
        case 'cancel':
          rename.cancel()
          restoreFocus()
          break
        case 'continue':
          rename.requestRefocus()
          break
      }
    },

    /**
     * Escape, Tab, or the editor losing focus: discard.
     *
     * `sessionId` is the session the editor was opened for. A superseded editor
     * blurs as it unmounts, and that blur must not end the session that has
     * taken its place.
     */
    handleRenameCancel(sessionId: RenameSessionId) {
      if (rename.isSuperseded(sessionId)) return
      if (suppressBlurCancel) {
        suppressBlurCancel = false
        return
      }
      // A click-away already sent the save; the blur it caused arrives right after
      // and must not cancel the session the save still owns.
      if (pendingCommit) return
      rename.cancel()
      restoreFocus()
    },

    handleRenameShakeEnd() {
      rename.clearShake()
    },
  }
}
