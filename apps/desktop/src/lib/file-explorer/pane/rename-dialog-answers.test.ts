import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { RenameValidityResult } from '#lib/tauri-commands/index.js'
import type { RenameTarget } from '../rename/rename-state.svelte'

const { moveToTrashSpy, refreshListingSpy, performRenameSpy, addToastSpy, getVolumesSpy } = vi.hoisted(() => ({
  moveToTrashSpy: vi.fn<(path: string, initiator: RenameTarget['initiator']) => Promise<void>>(),
  refreshListingSpy: vi.fn(),
  performRenameSpy: vi.fn<() => Promise<unknown>>(),
  addToastSpy: vi.fn(),
  getVolumesSpy: vi.fn(() => [{ id: 'root', supportsTrash: true }]),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  moveToTrash: moveToTrashSpy,
  refreshListing: refreshListingSpy,
  // The rename editor announces itself as a dialog when a session opens.
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/file-operations/mutation-error.js', () => ({
  asMutationError: (e: unknown) => (typeof e === 'object' && e !== null && 'type' in e ? e : null),
}))
vi.mock('#lib/file-operations/mutation-error-messages.js', () => ({
  renderMutationError: (failure: { type: string }, noun: string) => `${failure.type} (${noun})`,
}))
vi.mock('#lib/ui/toast/index.js', () => ({ addToastForPane: addToastSpy }))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: (k: string) => k }))
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: getVolumesSpy }))
vi.mock('../rename/rename-operations', () => ({ performRename: performRenameSpy }))

import { createRenameDialogAnswers } from './rename-dialog-answers.svelte'
import { createRenameState } from '../rename/rename-state.svelte'

const TARGET: RenameTarget = {
  path: '/dir/notes.txt',
  originalName: 'notes.txt',
  parentPath: '/dir',
  isDirectory: false,
  nameKind: 'file',
  initiator: 'user',
}

const CONFLICT: RenameValidityResult = {
  valid: true,
  error: null,
  hasConflict: true,
  isCaseOnlyRename: false,
  conflict: null,
  byMove: null,
}

function setup() {
  const rename = createRenameState()
  rename.activate(TARGET)
  const deps = {
    rename,
    paneId: 'left' as const,
    getListingId: () => 'listing-1',
    getCurrentPath: () => '/dir',
    getVolumeId: () => 'root',
    saveSkippingExtensionCheck: vi.fn(),
    land: vi.fn(),
    closeAndRefocus: vi.fn(),
  }
  return { rename, deps, answers: createRenameDialogAnswers(deps) }
}

/** Opens the conflict question for `taken.txt`, the name the person typed. */
function askTaken(answers: ReturnType<typeof setup>['answers']) {
  answers.askConflict({ type: 'conflict', validity: CONFLICT }, 'taken.txt')
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('the extension question', () => {
  it('opens with both extensions and asks until answered', () => {
    const { answers } = setup()
    expect(answers.isAsking).toBe(false)
    answers.askExtension({ type: 'extension-ask', oldExtension: 'txt', newExtension: 'md' }, TARGET)
    expect(answers.extensionDialogState).toEqual({ oldExtension: 'txt', newExtension: 'md', isDirectory: false })
    expect(answers.isAsking).toBe(true)
  })

  it('"Keep" puts the old extension back on what the person typed and hands focus back to the editor', () => {
    const { answers, rename } = setup()
    rename.setCurrentName('report.md')
    answers.askExtension({ type: 'extension-ask', oldExtension: 'txt', newExtension: 'md' }, TARGET)
    const focusBefore = rename.focusTrigger
    answers.keepOldExtension()
    expect(rename.currentName).toBe('report.txt')
    expect(rename.focusTrigger).not.toBe(focusBefore)
    expect(answers.extensionDialogState).toBeNull()
  })

  it('"Use" saves again past the question', () => {
    const { answers, deps } = setup()
    answers.askExtension({ type: 'extension-ask', oldExtension: 'txt', newExtension: 'md' }, TARGET)
    answers.useNewExtension()
    expect(deps.saveSkippingExtensionCheck).toHaveBeenCalledOnce()
    expect(answers.extensionDialogState).toBeNull()
  })
})

describe('the conflict question', () => {
  it('snapshots whether the clobbered file has a Trash to go to when it opens', () => {
    const { answers } = setup()
    askTaken(answers)
    expect(answers.conflictDialogState).toEqual({ validity: CONFLICT, trimmedName: 'taken.txt', supportsTrash: true })

    getVolumesSpy.mockReturnValueOnce([{ id: 'root', supportsTrash: false }])
    askTaken(answers)
    expect(answers.conflictDialogState?.supportsTrash).toBe(false)
  })

  it('dismiss clears both questions', () => {
    const { answers } = setup()
    askTaken(answers)
    answers.askExtension({ type: 'extension-ask', oldExtension: 'txt', newExtension: 'md' }, TARGET)
    answers.dismiss()
    expect(answers.isAsking).toBe(false)
  })

  it('"Cancel" ends the session, and "Continue editing" hands focus back', () => {
    const { answers, deps, rename } = setup()
    askTaken(answers)
    answers.resolveConflict('cancel')
    expect(deps.closeAndRefocus).toHaveBeenCalledOnce()

    askTaken(answers)
    const focusBefore = rename.focusTrigger
    answers.resolveConflict('continue')
    expect(rename.focusTrigger).not.toBe(focusBefore)
    expect(deps.closeAndRefocus).toHaveBeenCalledOnce()
  })

  it('an answer with no question open ends the session without renaming', () => {
    const { answers, deps } = setup()
    answers.resolveConflict('overwrite-delete')
    expect(performRenameSpy).not.toHaveBeenCalled()
    expect(deps.closeAndRefocus).toHaveBeenCalledOnce()
  })

  it('"Replace" renames over the other file and lands through the flow', async () => {
    const { answers, deps, rename } = setup()
    const result = { type: 'success', newName: 'taken.txt' }
    performRenameSpy.mockResolvedValueOnce(result)
    askTaken(answers)
    answers.resolveConflict('overwrite-delete')
    expect(performRenameSpy).toHaveBeenCalledWith(TARGET, 'taken.txt', true, 'root')
    await vi.waitFor(() => {
      expect(deps.land).toHaveBeenCalledWith(result, TARGET, 'taken.txt', rename.sessionId)
    })
  })

  it('"Move to Trash and replace" trashes the other file first, then renames over it', async () => {
    const { answers, deps } = setup()
    moveToTrashSpy.mockResolvedValueOnce(undefined)
    performRenameSpy.mockResolvedValueOnce({ type: 'success', newName: 'taken.txt' })
    askTaken(answers)
    answers.resolveConflict('overwrite-trash')
    await vi.waitFor(() => {
      expect(deps.land).toHaveBeenCalledOnce()
    })
    expect(moveToTrashSpy).toHaveBeenCalledWith('/dir/taken.txt', 'user')
    expect(moveToTrashSpy.mock.invocationCallOrder[0]).toBeLessThan(performRenameSpy.mock.invocationCallOrder[0])
  })

  it('a Trash that never confirmed warns that it may still land, tops the listing up, and ends the session', async () => {
    const { answers, deps } = setup()
    moveToTrashSpy.mockRejectedValueOnce({ type: 'timedOut' })
    askTaken(answers)
    answers.resolveConflict('overwrite-trash')
    await vi.waitFor(() => {
      expect(deps.closeAndRefocus).toHaveBeenCalledOnce()
    })
    expect(addToastSpy).toHaveBeenCalledWith('left', 'fileExplorer.pane.trashUnconfirmedToast', {
      level: 'warn',
      dismissal: 'persistent',
    })
    expect(refreshListingSpy).toHaveBeenCalledWith('listing-1', false)
    expect(performRenameSpy).not.toHaveBeenCalled()
  })

  it('a Trash that refused says why in the user’s words, renames nothing, and ends the session', async () => {
    const { answers, deps } = setup()
    moveToTrashSpy.mockRejectedValueOnce({ type: 'permissionDenied' })
    askTaken(answers)
    answers.resolveConflict('overwrite-trash')
    await vi.waitFor(() => {
      expect(deps.closeAndRefocus).toHaveBeenCalledOnce()
    })
    expect(addToastSpy).toHaveBeenCalledWith('left', 'permissionDenied (file)', { level: 'error' })
    expect(performRenameSpy).not.toHaveBeenCalled()
  })

  it('a refusal that arrives after the person moved on to another rename leaves that session alone', async () => {
    const { answers, deps, rename } = setup()
    let refuse!: (e: unknown) => void
    moveToTrashSpy.mockReturnValueOnce(
      new Promise((_, reject) => {
        refuse = reject
      }),
    )
    askTaken(answers)
    answers.resolveConflict('overwrite-trash')
    rename.activate({ ...TARGET, path: '/dir/other.txt', originalName: 'other.txt' })
    refuse(new Error('not a typed failure'))
    await vi.waitFor(() => {
      expect(addToastSpy).toHaveBeenCalledWith('left', 'unexpected (file)', { level: 'error' })
    })
    expect(deps.closeAndRefocus).not.toHaveBeenCalled()
  })
})
