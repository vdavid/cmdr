import { getFileAt, refreshListing } from '$lib/tauri-commands'
import { getIpcErrorMessage, isIpcError, moveToTrash, type RenameValidityResult } from '$lib/tauri-commands'
import { validateFilename, getExtension } from '$lib/utils/filename-validation'
import { cancelClickToRename } from '../rename/rename-activation'
import { executeRenameSave, performRename, checkPermission, type RenameResult } from '../rename/rename-operations'
import { getSetting } from '$lib/settings'
import type { RenameConflictResolution } from '../rename/rename-operations'
import { addToastForPane, dismissTransientToastsForPane, type ToastOriginPane } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import { pathInsideArchive } from './volume-capabilities'
import type { FileEntry } from '../types'
import type { StartRenameOptions } from './types'
import type { createRenameState, RenameSessionId, RenameTarget } from '../rename/rename-state.svelte'
import { resolveStepIndex, type RenameStepDirection } from '../rename/rename-step'
import { createSiblingNames, type ListingScope } from '../rename/sibling-names'

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
  /** Row the cursor is on, for the chained step's neighbour math. */
  getCursorIndex: () => number
  /** Cursor-addressable rows, `..` included when there is one. */
  getEffectiveTotalCount: () => number
  getHasParent: () => boolean
  /** A row read straight out of the loaded window; `undefined` when it isn't loaded. */
  getEntryAt: (index: number) => FileEntry | undefined
  /** Lands the cursor on a row and scrolls it into view. */
  moveCursorTo: (index: number) => void
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

  // The directory's own names, for the editor's conflict hint. Read once per
  // chain and patched as the chain's renames land, so hopping across 20 rows
  // pages the listing once rather than 20 times. Hint only: `decideStepFate`
  // never reads it (see `rename/sibling-names.ts`).
  const siblingNames = createSiblingNames()

  /** The listing the conflict hint is being checked against right now. */
  function currentScope(): ListingScope {
    return {
      listingId: deps.getListingId(),
      includeHidden: deps.getIncludeHidden(),
      parentPath: deps.getCurrentPath(),
      totalCount: deps.getTotalCount(),
    }
  }

  /**
   * Ends the chain the editor has been running: the next rename reads the
   * directory for itself.
   *
   * A chain is one run of the editor across rows. It opens with an activation
   * that no arrow asked for (F2, the menu, a click) and lasts until the editor
   * closes or another such activation replaces it.
   */
  function endChain() {
    siblingNames.clear()
  }

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
    // it so a bad proposal shows the red border immediately (the directory's names
    // may still be loading; they re-validate on the first keystroke). The editor
    // still selects the name-minus-extension on mount, so the user can accept it
    // with one keypress.
    if (initialName !== undefined) {
      rename.setCurrentName(initialName)
      const result = validateFilename(
        initialName,
        entry.name,
        deps.getCurrentPath(),
        siblingNames.names,
        effectiveExtensionPolicy(),
      )
      rename.setValidation(result)
    }

    // Scoped to the listing rather than to the session, so a read that lands
    // after the chain has hopped on still answers for the right directory, and
    // a chained activation reuses what the chain already read.
    void siblingNames.ensure(currentScope())

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

  /**
   * Opens the editor on `entry` itself, with no cursor round-trip.
   *
   * `startRename` activates on whatever `getEntryUnderCursor()` reports, and
   * that value is filled by an async read keyed on the cursor index: right
   * after a chained step moves the cursor it still names the file the chain
   * just left, so activating through it would write the next name onto that
   * file. Taking the entry directly makes the target path-bound by
   * construction.
   */
  function startRenameOnEntry(entry: FileEntry): void {
    clearPendingRenameActivation()
    // The next session validates its extension under the user's own setting;
    // only an auto-started rename (paste-clipboard-as-file) suppresses that.
    suppressExtensionWarningOnce = false
    activateRename(entry)
  }

  /** Reads a row from the backend when it sits outside the loaded window. */
  async function fetchEntryAt(index: number): Promise<FileEntry | undefined> {
    const backendIndex = deps.getHasParent() ? index - 1 : index
    try {
      return (await getFileAt(deps.getListingId(), backendIndex, deps.getIncludeHidden())) ?? undefined
    } catch {
      return undefined
    }
  }

  /**
   * Decides what becomes of the edit the user is stepping away from.
   *
   * A name the frontend already grades as unusable is dropped here instead of
   * sent, so a chain doesn't spend a round trip to be told what it knew. That
   * covers the extension-change policy on its own: under "no" a changed
   * extension IS a validation error, so skipping the dialog (which a chain must)
   * never turns into overriding the setting. Under "ask" it grades as fine and
   * commits, and the operation log is the way back from a fumbled extension.
   *
   * A CONFLICT is deliberately not decided here. The frontend's conflict signal
   * is only a warning, computed against the directory names the chain read when
   * it started, and a chain rewrites the directory as it runs: those names can
   * call a name taken that is perfectly free, and dropping the edit on that
   * would throw away what the user typed. The backend has the authoritative
   * answer, and `reportSupersededResult` acts on that one.
   *
   * Nothing has to actively discard: the activation that follows resets the
   * editor, so an edit that isn't sent is simply gone.
   */
  function decideStepFate(): void {
    const target = rename.target
    if (!target || !rename.hasChanged()) return
    if (rename.severity === 'error') {
      toastKeptName(target.originalName, rename.validation.message)
      return
    }
    void executeFlow(true)
  }

  // One toast for every name a chain didn't apply, replaced in place as more
  // arrive. Reusing the id is what keeps the count honest: the store holds five
  // toasts and silently DROPS a new one when they're all persistent, so a toast
  // per kept name would lose everything past the fifth without a trace.
  const keptNamesToastId = `rename-kept-names-${deps.paneId}`
  // Names counted by the toast currently on screen: dismissing it is the user
  // saying they've read it, and the next one starts over.
  let keptNamesCount = 0

  /**
   * Says which files kept their names when chained renames didn't apply.
   *
   * Persistent on purpose: `handleRenameInput` clears this pane's transient
   * toasts on every keystroke, which is exactly when the user is typing the next
   * name, so a transient one would be gone before it was read.
   *
   * The newest file is the one named, with the reason it kept its name; the
   * others become a count. Holding the arrow through a directory where a dozen
   * names clash is one message that grows, not a dozen fighting for five slots.
   */
  function toastKeptName(originalName: string, reason: string): void {
    keptNamesCount += 1
    const others = keptNamesCount - 1
    const content =
      others === 0
        ? tString('fileExplorer.rename.chainKeptOriginalName', { reason, name: originalName })
        : tString('fileExplorer.rename.chainKeptOriginalNameAndOthers', {
            reason,
            name: originalName,
            others,
            othersText: formatInteger(others),
          })
    addToast(content, {
      level: 'warn',
      dismissal: 'persistent',
      id: keptNamesToastId,
      onDismiss: () => {
        keptNamesCount = 0
      },
    })
  }

  // One toast for every rename this pane couldn't confirm, on the same terms as
  // the kept names: one id, replaced in place. Two things force it. A toast each
  // is dropped past the fifth; and five persistent toasts fill the stack, which
  // would leave `toastKeptName` unable to say anything at all, on exactly the
  // slow volumes where both happen at once.
  const unconfirmedToastId = `rename-unconfirmed-${deps.paneId}`
  // Renames counted by the toast currently on screen, zeroed when the user
  // dismisses it, same as the kept names.
  let unconfirmedCount = 0

  // A volume too slow to answer a rename must not then be asked to list the
  // directory once per unanswered rename. The refresh waits out a quiet spell
  // and runs once; landing AFTER the last straggler is also what makes the
  // listing show the settled truth rather than a half-finished chain.
  const UNCONFIRMED_REFRESH_QUIET_MS = 1000
  let unconfirmedRefreshTimer: ReturnType<typeof setTimeout> | null = null

  function scheduleUnconfirmedRefresh(): void {
    if (unconfirmedRefreshTimer !== null) clearTimeout(unconfirmedRefreshTimer)
    unconfirmedRefreshTimer = setTimeout(() => {
      unconfirmedRefreshTimer = null
      // Read at fire time: the pane may have moved on, and the listing worth
      // refreshing is the one it is showing now.
      void refreshListing(deps.getListingId())
    }, UNCONFIRMED_REFRESH_QUIET_MS)
  }

  /**
   * Says which renames the volume never confirmed, and refreshes to find out.
   *
   * A timeout is NOT a refusal: the rename may well have landed on disk. So this
   * never says the file kept its name, and stays a separate message from
   * `toastKeptName` however tempting the shared shape looks.
   *
   * Persistent for the same reason: the next keystroke clears this pane's
   * transient toasts, and the user is typing the next name right then.
   */
  function toastUnconfirmedRename(name: string): void {
    unconfirmedCount += 1
    const others = unconfirmedCount - 1
    const content =
      others === 0
        ? tString('fileExplorer.rename.unconfirmed', { name })
        : tString('fileExplorer.rename.unconfirmedAndOthers', {
            name,
            others,
            othersText: formatInteger(others),
          })
    addToast(content, {
      level: 'warn',
      dismissal: 'persistent',
      id: unconfirmedToastId,
      onDismiss: () => {
        unconfirmedCount = 0
      },
    })
    scheduleUnconfirmedRefresh()
  }

  /**
   * Settles the current edit and reopens the editor on `entry`.
   *
   * The save goes FIRST, and unawaited: `executeFlow` reads the target, the
   * typed name, and the session id synchronously before it awaits anything, so
   * firing it here is what tags it with the session that typed it. Activating
   * first would hand the in-flight save the NEW session's id and blind every
   * supersession guard at once.
   *
   * It skips the extension check because a dialog can't stop a chain: it would
   * ask about a file the user has already moved past.
   */
  function stepTo(index: number, entry: FileEntry): void {
    decideStepFate()
    deps.moveCursorTo(index)
    startRenameOnEntry(entry)
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
   *
   * `target` and `trimmedName` are the ones the save was sent with, so the toast
   * can name the file that kept its name rather than whichever one the editor
   * has since landed on.
   */
  function reportSupersededResult(result: RenameResult, target: RenameTarget, trimmedName: string) {
    switch (result.type) {
      case 'success':
        // The chain's own doing: the directory the hint checks against just
        // changed, and nothing else will tell it.
        siblingNames.applyRename(target.parentPath, target.originalName, result.newName)
        toastIfHiddenAfterRename(result.newName)
        break
      case 'error':
        // The file definitely kept its name, so it belongs in the same running
        // toast as every other name the chain dropped. A plain toast here would
        // be transient, and the next keystroke wipes this pane's transient
        // toasts: a chain over a volume that refuses every rename would report
        // nothing at all.
        toastKeptName(target.originalName, result.message)
        break
      case 'timeout':
        toastUnconfirmedRename(target.originalName)
        break
      case 'conflict':
        // The only authority on a conflict, and the chain must not stop to ask:
        // the name is dropped, and the toast is how the user learns it was.
        toastKeptName(target.originalName, tString('fileOperations.validation.conflict', { name: trimmedName }))
        break
      case 'noop':
      case 'extension-ask':
        // Nothing happened on disk, and the extension question can't be put to a
        // user who has moved on: the edit is dropped.
        break
    }
  }

  function handleRenameResult(
    result: RenameResult,
    target: RenameTarget,
    trimmedName: string,
    sessionId: RenameSessionId,
  ) {
    if (rename.isSuperseded(sessionId)) {
      reportSupersededResult(result, target, trimmedName)
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
        // The same aggregated toast the chain uses: a chain's last rename ends
        // here rather than superseded, and its timeout belongs in the running
        // count with the others.
        toastUnconfirmedRename(target.originalName)
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
    endChain()
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
    handleRenameResult(result, target, trimmedName, sessionId)
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
      // A fresh startRename supersedes any pending auto-activation poll, and
      // opens a new chain: this is the activation no arrow asked for.
      clearPendingRenameActivation()
      endChain()

      // Scoped to this rename session; reset when it ends (finalize/cancel).
      suppressExtensionWarningOnce = options?.suppressExtensionWarning ?? false
      const expectedName = options?.expectedName
      const initialName = options?.initialName

      // Activate ONLY on the intended entry. The permission check (skipped for MTP
      // and archive-inner paths) and the directory-name read live in `activateRename`.
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
      endChain()
      rename.cancel()
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
        siblingNames.names,
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
              handleRenameResult(result, target, trimmedName, sessionId)
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
            handleRenameResult(result, target, trimmedName, sessionId)
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
      endChain()
      rename.cancel()
      restoreFocus()
    },

    /**
     * ArrowDown / ArrowUp inside the editor: send the current name off to be
     * saved and reopen the editor on the neighbouring row, so a run of files
     * gets renamed in one keyboard flow.
     *
     * The neighbour is captured BEFORE the save goes out, and by path: the
     * rename may re-sort the listing and carry the renamed file far away, and
     * the row the user meant is the one that sat beside the editor when they
     * pressed the key.
     *
     * At either end of the listing there's nothing to step to, and the key does
     * nothing at all: no commit, no discard, the editor stays open with the edit
     * intact. Nothing rate-limits key repeat; session ids are what keep a burst
     * of steps from crossing each other's results.
     */
    handleRenameStep(direction: RenameStepDirection, sessionId: RenameSessionId) {
      if (!rename.active || rename.isSuperseded(sessionId)) return

      const index = resolveStepIndex(direction, {
        cursorIndex: deps.getCursorIndex(),
        rowCount: deps.getEffectiveTotalCount(),
        hasParent: deps.getHasParent(),
      })
      if (index === undefined) return

      const entry = deps.getEntryAt(index)
      if (entry) {
        stepTo(index, entry)
        return
      }

      // The neighbour has scrolled out of the loaded window. Fetching it costs a
      // round trip, in which the user can end the rename or start another one,
      // so the session has to answer for itself again before the step lands.
      void fetchEntryAt(index).then((fetched) => {
        if (!fetched || !rename.active || rename.isSuperseded(sessionId)) return
        stepTo(index, fetched)
      })
    },

    handleRenameShakeEnd() {
      rename.clearShake()
    },
  }
}
