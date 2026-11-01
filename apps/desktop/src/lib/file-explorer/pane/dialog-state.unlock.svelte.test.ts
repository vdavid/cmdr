/**
 * The unlock prompt's interception + follow-up paths in `createDialogState`.
 *
 * A copy/move whose source is inside a locked container surfaces as a
 * `WriteOperationError` of type `archive_needs_password`. `handleTransferError`
 * must intercept exactly that variant BEFORE the generic error dialog, show the
 * password prompt, and keep `transferProgressProps` alive so a successful unlock
 * re-dispatches the same op and a cancel settles it cleanly. Other variants must
 * still route to the generic error dialog.
 *
 * An archive stores the password unchecked and closes at once. A restic repo
 * CHECKS it with the prompt still up (verify before close): a wrong one re-arms
 * the prompt in place, and Cancel mid-check closes at once and drops the check.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createDialogState } from './dialog-state.svelte'
import type { TransferProgressPropsData } from './dialog-props'
import type { WriteOperationError } from '../types'
import type { FilePaneAPI } from './types'
import type { UnlockOutcome } from '#lib/ipc/bindings.js'
import { UnlockFailure } from '#lib/tauri-commands/unlock.js'

const {
  setArchivePassword,
  unlockContainer,
  cancelContainerUnlock,
  forgetContainerPassword,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
} = vi.hoisted(() => ({
  setArchivePassword: vi.fn(() => Promise.resolve()),
  unlockContainer: vi.fn<(request: unknown) => Promise<UnlockOutcome>>(),
  cancelContainerUnlock: vi.fn(() => Promise.resolve()),
  forgetContainerPassword: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordPrompt: vi.fn(() => Promise.resolve()),
  notifyArchivePasswordDismissed: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/tauri-commands/index.js', async () => ({
  refreshListing: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  setArchivePassword,
  unlockContainer,
  cancelContainerUnlock,
  forgetContainerPassword,
  // The real class, so the flow's `failureOf` recognizes what the stub throws.
  UnlockFailure: (
    await vi.importActual<typeof import('#lib/tauri-commands/unlock.js')>('#lib/tauri-commands/unlock.js')
  ).UnlockFailure,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: vi.fn() }))
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({ removeEntryFromAllSnapshots: vi.fn() }))
vi.mock('#lib/file-operations/mkdir/new-folder-operations.js', () => ({ moveCursorToNewFolder: vi.fn() }))

/** Minimal `FilePaneAPI` stub plus the spies its transfer-path members expose. */
function makePaneRef() {
  const spies = {
    clearSelection: vi.fn(),
    selectAll: vi.fn(),
    snapshotSelectionForOperation: vi.fn(() => Promise.resolve()),
    clearOperationSnapshot: vi.fn(() => null),
    getListingId: vi.fn(() => 'listing-1'),
    // The pane is where the operation was born, which is what the settled-transfer
    // tail checks before it touches a selection: a pane that has navigated since
    // holds one the user made somewhere else.
    getCurrentPath: vi.fn(() => '/Users/me/secret.zip/inner'),
    refreshVolumeSpace: vi.fn(() => Promise.resolve()),
  }
  return { ref: spies as unknown as FilePaneAPI, spies }
}

const onRefocus = vi.fn()

function makeState() {
  const rightPane = makePaneRef()
  const leftPane = makePaneRef()
  const dialogs = createDialogState({
    getLeftPaneRef: () => leftPane.ref,
    getRightPaneRef: () => rightPane.ref,
    getFocusedPaneRef: () => rightPane.ref,
    getFocusedPaneSide: () => 'right',
    getShowHiddenFiles: () => false,
    // No pane navigation in these suites; the trash toast is the only consumer.
    getExplorer: () => undefined,
    onRefocus,
    skipsConfirmations: () => false,
    onOpenInEditor: vi.fn(),
  })
  return { dialogs, rightPane, leftPane }
}

/** A copy op sourced from inside an encrypted zip on the `root` volume. */
function copyProps(): TransferProgressPropsData {
  return {
    initiator: 'user',
    operationType: 'copy',
    sourcePaths: ['/Users/me/secret.zip/inner/report.pdf'],
    sourceFolderPath: '/Users/me/secret.zip/inner',
    sourcePaneSide: 'right',
    destinationPath: '/Users/me/out',
    direction: 'left',
    previewId: 'preview-1',
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    conflictResolution: 'stop',
    duplicateFollowUp: 'nothing',
  }
}

const needsPassword = (wrongAttempt: boolean): WriteOperationError => ({
  type: 'archive_needs_password',
  path: '/Users/me/secret.zip/inner/report.pdf',
  wrongAttempt,
})

beforeEach(() => {
  vi.clearAllMocks()
})

describe('archive-password interception', () => {
  it('intercepts archive_needs_password: prompts instead of the generic error dialog', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferError(needsPassword(false), null)

    expect(dialogs.showUnlockDialog).toBe(true)
    expect(dialogs.showTransferErrorDialog).toBe(false)
    // The progress dialog is unmounted but its props stay alive for a retry.
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).not.toBeNull()

    const props = dialogs.unlockProps
    expect(props).toEqual({
      container: 'archive',
      name: 'secret.zip',
      wrongAttempt: false,
      parentVolumeId: 'root',
      path: '/Users/me/secret.zip/inner/report.pdf',
      mode: 'transfer',
      checking: false,
      unreachable: false,
      rejections: 0,
    })
  })

  it('passes wrongAttempt through so the re-prompt shows its distinct copy', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferError(needsPassword(true), null)

    expect(dialogs.unlockProps?.wrongAttempt).toBe(true)
  })

  it('does NOT intercept other error variants (they still show the error dialog)', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    const ioError: WriteOperationError = { type: 'io_error', path: '/x', message: 'boom' }
    dialogs.handleTransferError(ioError, null)

    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.showTransferErrorDialog).toBe(true)
    expect(dialogs.transferProgressProps).toBeNull()
  })
})

describe('archive-password submit → re-dispatch', () => {
  it('stores the password and re-dispatches the same op with a fresh scan', async () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false), null)

    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    expect(setArchivePassword).toHaveBeenCalledWith('root', '/Users/me/secret.zip/inner/report.pdf', 'hunter2')
    // The prompt closes immediately; the re-dispatch runs after the store resolves.
    expect(dialogs.showUnlockDialog).toBe(false)
    await vi.waitFor(() => {
      expect(dialogs.showTransferProgressDialog).toBe(true)
    })
    // The consumed preview is dropped so the retry re-scans the archive index.
    expect(dialogs.transferProgressProps?.previewId).toBeNull()
    // Same operation otherwise.
    expect(dialogs.transferProgressProps?.sourcePaths).toEqual(['/Users/me/secret.zip/inner/report.pdf'])
  })
})

describe('archive-password cancel → settle', () => {
  it('forgets the password and settles like a dismissed transfer (nothing stuck)', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false), null)

    dialogs.handleUnlockCancel()

    expect(forgetContainerPassword).toHaveBeenCalledWith('root', '/Users/me/secret.zip/inner/report.pdf')
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.unlockProps).toBeNull()
    expect(dialogs.transferProgressProps).toBeNull()
    expect(rightPane.spies.clearSelection).toHaveBeenCalled()
    expect(onRefocus).toHaveBeenCalled()
  })
})

// Browse path: a directory listing of a header-encrypted archive needs the
// password even to list it (no transfer op involved). `showUnlockForBrowse`
// opens the prompt; submit stores the password and re-lists via `retry`; cancel
// forgets the password and leaves the fallback error pane in place.
describe('archive-password browse path', () => {
  const browseInfo = (wrongAttempt: boolean, retry = vi.fn()) => ({
    volumeId: 'root',
    path: '/Users/me/locked.7z',
    wrongAttempt,
    retry,
  })

  it('opens the prompt in browse mode from a listing failure', () => {
    const { dialogs } = makeState()
    dialogs.showUnlockForBrowse(browseInfo(false))

    expect(dialogs.showUnlockDialog).toBe(true)
    // Browse mode carries no transfer op.
    expect(dialogs.transferProgressProps).toBeNull()
    expect(dialogs.unlockProps).toMatchObject({
      container: 'archive',
      name: 'locked.7z',
      wrongAttempt: false,
      parentVolumeId: 'root',
      path: '/Users/me/locked.7z',
      mode: 'browse',
    })
  })

  it('submit stores the password and re-lists via retry', async () => {
    const { dialogs } = makeState()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse(browseInfo(false, retry))

    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    expect(setArchivePassword).toHaveBeenCalledWith('root', '/Users/me/locked.7z', 'hunter2')
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.unlockProps).toBeNull()
    // The re-list runs after the store resolves.
    await vi.waitFor(() => {
      expect(retry).toHaveBeenCalled()
    })
    // No transfer dialog is ever involved on the browse path.
    expect(dialogs.showTransferProgressDialog).toBe(false)
  })

  it('cancel forgets the password without touching the transfer path', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.showUnlockForBrowse(browseInfo(false))

    dialogs.handleUnlockCancel()

    expect(forgetContainerPassword).toHaveBeenCalledWith('root', '/Users/me/locked.7z')
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.unlockProps).toBeNull()
    // No transfer op to settle: the source pane's selection is left untouched.
    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
    expect(onRefocus).toHaveBeenCalled()
  })

  it('passes wrongAttempt through so a rejected password re-prompts with distinct copy', () => {
    const { dialogs } = makeState()
    dialogs.showUnlockForBrowse(browseInfo(true))

    expect(dialogs.unlockProps?.wrongAttempt).toBe(true)
  })
})

// The prompt used to be visible only to whoever was looking at the screen: MCP
// saw a bare `- type: archive-password` and could name nothing. Every raise
// mirrors what is being ASKED to the backend, and every dismissal clears it.
describe('archive-password mirror for cmdr://state', () => {
  it('mirrors what a transfer prompt is asking, and never the password', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())

    dialogs.handleTransferError(needsPassword(true), null)

    expect(notifyArchivePasswordPrompt).toHaveBeenCalledWith({
      archiveName: 'secret.zip',
      archivePath: '/Users/me/secret.zip/inner/report.pdf',
      parentVolumeId: 'root',
      container: 'archive',
      mode: 'transfer',
      wrongAttempt: true,
      operationId: null,
    })
  })

  it('mirrors a browse prompt with no operation attached', () => {
    const { dialogs } = makeState()
    dialogs.showUnlockForBrowse({
      volumeId: 'root',
      path: '/Users/me/locked.7z',
      wrongAttempt: false,
      retry: vi.fn(),
    })

    expect(notifyArchivePasswordPrompt).toHaveBeenCalledWith(
      expect.objectContaining({ mode: 'browse', operationId: null }),
    )
  })

  it('clears the mirror on cancel, so nothing advertises a question that is gone', () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false), null)

    dialogs.handleUnlockCancel()

    expect(notifyArchivePasswordDismissed).toHaveBeenCalled()
  })
})

// The boundary the whole MCP surface is shaped around: an agent may supply the
// password, but it must never be the thing that starts the write.
describe('archive-password supplied over MCP', () => {
  it('settles a transfer instead of re-dispatching it, so no write starts', () => {
    const { dialogs, rightPane } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false), null)

    void dialogs.confirmOpenDialog('archive-password')

    // ⭐ The proof. A person's submit re-shows the progress dialog on a fresh
    // dispatch; this must not, or extraction would be the one write that skips
    // the confirmation and the token every other copy goes through.
    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
    // And it settles cleanly rather than leaving the pane looking mid-copy.
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(rightPane.spies.clearSelection).toHaveBeenCalled()
    expect(onRefocus).toHaveBeenCalled()
  })

  it('does NOT forget the password it was just given', () => {
    // Cancel clears the stored password; this path is the opposite of a cancel,
    // and clearing here would make the follow-up copy prompt all over again.
    const { dialogs } = makeState()
    dialogs.startTransferProgress(copyProps())
    dialogs.handleTransferError(needsPassword(false), null)

    void dialogs.confirmOpenDialog('archive-password')

    expect(forgetContainerPassword).not.toHaveBeenCalled()
    // The backend stored it before confirming, so the frontend stores nothing:
    // the secret never crosses into the webview.
    expect(setArchivePassword).not.toHaveBeenCalled()
  })

  it('completes a browse by re-listing, because a listing is a read', () => {
    const { dialogs } = makeState()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse({
      volumeId: 'root',
      path: '/Users/me/locked.7z',
      wrongAttempt: false,
      retry,
    })

    void dialogs.confirmOpenDialog('archive-password')

    expect(retry).toHaveBeenCalled()
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(notifyArchivePasswordDismissed).toHaveBeenCalled()
  })

  it('is a no-op when no prompt is up', () => {
    const { dialogs } = makeState()
    void dialogs.confirmOpenDialog('archive-password')

    expect(dialogs.showUnlockDialog).toBe(false)
    expect(notifyArchivePasswordDismissed).not.toHaveBeenCalled()
  })
})

// ── Restic repos: the password is checked before the prompt closes ──────────

const REPO_VIEW = '/Volumes/naspi/backups/m3-restic/Browse snapshots'

/** A promise the test settles by hand, standing in for a check in flight. */
function pendingUnlock() {
  let resolve!: (outcome: UnlockOutcome) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<UnlockOutcome>((res, rej) => {
    resolve = res
    reject = rej
  })
  unlockContainer.mockReturnValueOnce(promise)
  return { resolve, reject }
}

/** The `unlockId` the flow named its N-th check with. */
function unlockIdOf(call = 0): string {
  return (unlockContainer.mock.calls[call][0] as { unlockId: string }).unlockId
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('restic repo browse unlock', () => {
  const repoBrowse = (retry = vi.fn()) => ({
    volumeId: 'naspi',
    path: `${REPO_VIEW}/latest`,
    wrongAttempt: false,
    retry,
  })

  it('names the repo folder and mirrors the prompt as a repo', () => {
    const { dialogs } = makeState()
    dialogs.showUnlockForBrowse(repoBrowse())

    expect(dialogs.unlockProps).toMatchObject({ container: 'resticRepo', name: 'm3-restic', mode: 'browse' })
    expect(notifyArchivePasswordPrompt).toHaveBeenCalledWith(
      expect.objectContaining({
        container: 'resticRepo',
        archiveName: 'm3-restic',
        archivePath: `${REPO_VIEW}/latest`,
      }),
    )
  })

  it('keeps the prompt up while the repo checks the password, and closes and re-lists once it opens', async () => {
    const { dialogs } = makeState()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse(repoBrowse(retry))
    const check = pendingUnlock()

    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    expect(unlockContainer).toHaveBeenCalledWith({
      parentVolumeId: 'naspi',
      path: `${REPO_VIEW}/latest`,
      password: 'hunter2',
      remember: true,
      unlockId: expect.any(String) as string,
    })
    // ⭐ Verify before close: nothing closes or re-lists on the strength of a guess.
    expect(dialogs.showUnlockDialog).toBe(true)
    expect(dialogs.unlockProps?.checking).toBe(true)
    expect(retry).not.toHaveBeenCalled()
    // A repo password never goes through the archive's store-blind path.
    expect(setArchivePassword).not.toHaveBeenCalled()

    check.resolve('unlocked')
    await vi.waitFor(() => {
      expect(dialogs.showUnlockDialog).toBe(false)
    })
    expect(retry).toHaveBeenCalledOnce()
    expect(notifyArchivePasswordDismissed).toHaveBeenCalled()
  })

  it('re-arms the prompt in place on a wrong password, then opens on the right one', async () => {
    const { dialogs } = makeState()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse(repoBrowse(retry))

    const first = pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'wrong', remember: true })
    first.resolve('wrongPassword')
    await vi.waitFor(() => {
      expect(dialogs.unlockProps?.checking).toBe(false)
    })

    expect(dialogs.showUnlockDialog).toBe(true)
    expect(dialogs.unlockProps).toMatchObject({ wrongAttempt: true, rejections: 1, unreachable: false })
    expect(retry).not.toHaveBeenCalled()
    // The mirror says the prompt is asking again, so an agent reading state sees it.
    expect(notifyArchivePasswordPrompt).toHaveBeenLastCalledWith(expect.objectContaining({ wrongAttempt: true }))

    const second = pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'right', remember: false })
    expect(unlockContainer).toHaveBeenLastCalledWith(expect.objectContaining({ password: 'right', remember: false }))
    second.resolve('unlocked')
    await vi.waitFor(() => {
      expect(dialogs.showUnlockDialog).toBe(false)
    })
    expect(retry).toHaveBeenCalledOnce()
  })

  it('ignores a second submit while a check is running', () => {
    const { dialogs } = makeState()
    dialogs.showUnlockForBrowse(repoBrowse())
    pendingUnlock()

    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    expect(unlockContainer).toHaveBeenCalledOnce()
  })

  it('Cancel mid-check closes at once, drops the check by its id, and ignores what it answers later', async () => {
    const { dialogs, rightPane } = makeState()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse(repoBrowse(retry))
    const check = pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    dialogs.handleUnlockCancel()

    // At once, not when the backend gets round to answering.
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.unlockProps).toBeNull()
    expect(cancelContainerUnlock).toHaveBeenCalledWith(unlockIdOf())
    expect(onRefocus).toHaveBeenCalled()
    // Forgotten only after the cancel landed, so a check finishing in between
    // can't keep the password behind the forget.
    await vi.waitFor(() => {
      expect(forgetContainerPassword).toHaveBeenCalledWith('naspi', `${REPO_VIEW}/latest`)
    })

    check.resolve('unlocked')
    await flush()
    expect(retry).not.toHaveBeenCalled()
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(rightPane.spies.clearSelection).not.toHaveBeenCalled()
  })

  it('keeps the prompt up with the password when the repo couldn’t be reached', async () => {
    const { dialogs } = makeState()
    const retry = vi.fn()
    dialogs.showUnlockForBrowse(repoBrowse(retry))
    const check = pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    check.reject(new UnlockFailure({ type: 'timedOut' }))
    await vi.waitFor(() => {
      expect(dialogs.unlockProps?.checking).toBe(false)
    })

    expect(dialogs.showUnlockDialog).toBe(true)
    // Not a wrong password: it said nothing about the password at all.
    expect(dialogs.unlockProps).toMatchObject({ unreachable: true, wrongAttempt: false, rejections: 0 })
    expect(retry).not.toHaveBeenCalled()
  })

  it('a cancel answer from the backend after the prompt closed leaves everything alone', async () => {
    const { dialogs } = makeState()
    dialogs.showUnlockForBrowse(repoBrowse())
    const check = pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })
    dialogs.handleUnlockCancel()

    check.reject(new UnlockFailure({ type: 'cancelled' }))
    await flush()

    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.unlockProps).toBeNull()
  })
})

describe('restic repo transfer unlock', () => {
  function repoCopyProps(): TransferProgressPropsData {
    return {
      ...copyProps(),
      sourcePaths: [`${REPO_VIEW}/latest/Users/me/report.pdf`],
      sourceFolderPath: `${REPO_VIEW}/latest/Users/me`,
      sourceVolumeId: 'naspi',
    }
  }

  const repoNeedsPassword: WriteOperationError = {
    type: 'archive_needs_password',
    path: `${REPO_VIEW}/latest/Users/me/report.pdf`,
    wrongAttempt: false,
  }

  it('checks the password, then re-dispatches the same copy with a fresh scan', async () => {
    const { dialogs } = makeState()
    dialogs.startTransferProgress(repoCopyProps())
    dialogs.handleTransferError(repoNeedsPassword, null)
    expect(dialogs.unlockProps).toMatchObject({ container: 'resticRepo', name: 'm3-restic', mode: 'transfer' })

    const check = pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })
    expect(dialogs.showTransferProgressDialog).toBe(false)

    check.resolve('unlocked')
    await vi.waitFor(() => {
      expect(dialogs.showTransferProgressDialog).toBe(true)
    })
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.transferProgressProps?.previewId).toBeNull()
  })

  it('Cancel mid-check settles the copy like a dismissed one', () => {
    const { dialogs, rightPane } = makeState()
    rightPane.spies.getCurrentPath.mockReturnValue(`${REPO_VIEW}/latest/Users/me`)
    dialogs.startTransferProgress(repoCopyProps())
    dialogs.handleTransferError(repoNeedsPassword, null)
    pendingUnlock()
    dialogs.handleUnlockSubmit({ password: 'hunter2', remember: true })

    dialogs.handleUnlockCancel()

    expect(cancelContainerUnlock).toHaveBeenCalledWith(unlockIdOf())
    expect(dialogs.showUnlockDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
    expect(rightPane.spies.clearSelection).toHaveBeenCalled()
  })

  it('an agent’s unlock of a repo settles the copy rather than starting it', () => {
    // The backend checked the password before confirming; the boundary is the
    // same as an archive's: an agent never starts the write.
    const { dialogs } = makeState()
    dialogs.startTransferProgress(repoCopyProps())
    dialogs.handleTransferError(repoNeedsPassword, null)

    void dialogs.confirmOpenDialog('archive-password')

    expect(dialogs.showTransferProgressDialog).toBe(false)
    expect(dialogs.transferProgressProps).toBeNull()
    expect(unlockContainer).not.toHaveBeenCalled()
  })
})
