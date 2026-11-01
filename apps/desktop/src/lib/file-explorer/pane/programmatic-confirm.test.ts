import { describe, expect, it, vi } from 'vitest'
import { createProgrammaticConfirm, type ProgrammaticConfirmDeps } from './programmatic-confirm'
import type { ConfirmPress } from './dialog-props'

function makeDeps(open: 'transfer' | 'delete' | 'newFolder' | 'newFile' | 'none'): ProgrammaticConfirmDeps {
  return {
    isTransferDialogOpen: () => open === 'transfer',
    isDeleteDialogOpen: () => open === 'delete',
    isNewFolderDialogOpen: () => open === 'newFolder',
    isNewFileDialogOpen: () => open === 'newFile',
    isArchivePasswordOpen: () => false,
    supplyStoredPassword: vi.fn(),
  }
}

const answering = (press: ConfirmPress) => vi.fn(() => Promise.resolve(press))

describe('confirmOpenDialog on the transfer dialog, and its policy', () => {
  it('hands the dialog no policy when none was named, so it keeps the one it shows', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('transfer'))
    const press = answering('pressed')
    confirmer.registerTransferConfirmer(press)

    await confirmer.confirmOpenDialog('transfer-confirmation')

    expect(press).toHaveBeenCalledWith(undefined, {})
  })

  it('starts nothing under a policy it has never heard of', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('transfer'))
    const press = answering('pressed')
    confirmer.registerTransferConfirmer(press)

    const verdict = await confirmer.confirmOpenDialog('transfer-confirmation', 'keep_both')

    expect(press).not.toHaveBeenCalled()
    expect(verdict).toEqual({ pressed: false, refusal: 'notStarted' })
  })
})

describe('confirmOpenDialog on a naming dialog', () => {
  it.each([
    ['newFolder', 'mkdir-confirmation', 'registerNewFolderConfirmer'],
    ['newFile', 'new-file-confirmation', 'registerNewFileConfirmer'],
  ] as const)('presses %s with the name and answers with how it ended', async (open, type, register) => {
    const confirmer = createProgrammaticConfirm(makeDeps(open))
    const press = vi.fn(() => Promise.resolve({ kind: 'done' as const }))
    confirmer[register](press)

    const verdict = await confirmer.confirmOpenDialog(type, undefined, { name: 'notes' })

    expect(press).toHaveBeenCalledWith('notes')
    expect(verdict).toEqual({ pressed: true, committed: true, stillRunning: false })
  })

  it('carries a refused name with its reason', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('newFile'))
    confirmer.registerNewFileConfirmer(() => Promise.resolve({ kind: 'refused', reason: 'Taken.' }))

    const verdict = await confirmer.confirmOpenDialog('new-file-confirmation', undefined, { name: 'a' })

    expect(verdict).toEqual({ pressed: false, refusal: 'nameRefused', reason: 'Taken.' })
  })

  it('is not ready before the dialog registered its confirm', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('newFolder'))

    const verdict = await confirmer.confirmOpenDialog('mkdir-confirmation', undefined, { name: 'a' })

    expect(verdict).toEqual({ pressed: false, refusal: 'notReady' })
  })
})

describe('confirmOpenDialog on the transfer dialog, in the background', () => {
  it('presses the Background button under the named policy, with the round-trip id', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('transfer'))
    const press = answering('pressed')
    confirmer.registerTransferConfirmer(press)

    const verdict = await confirmer.confirmOpenDialog('transfer-confirmation', 'overwrite_all', {
      startInBackground: true,
      mcpRequestId: 'req-1',
    })

    expect(press).toHaveBeenCalledWith('overwrite', { startInBackground: true, mcpRequestId: 'req-1' })
    expect(verdict).toEqual({ pressed: true })
  })

  it('says, typed, when the dialog took the press and started nothing', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('transfer'))
    confirmer.registerTransferConfirmer(answering('notStarted'))

    const verdict = await confirmer.confirmOpenDialog('transfer-confirmation', 'skip_all')

    expect(verdict).toEqual({ pressed: false, refusal: 'notStarted' })
  })
})

describe('confirmOpenDialog on the delete dialog', () => {
  it('presses the mounted dialog’s own confirm', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    const press = answering('pressed')
    confirmer.registerDeleteConfirmer(press)

    await confirmer.confirmOpenDialog('delete-confirmation')

    expect(press).toHaveBeenCalledOnce()
  })

  it('confirms nothing while the dialog is open but not mounted yet', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))

    await expect(confirmer.confirmOpenDialog('delete-confirmation')).resolves.toEqual({
      pressed: false,
      refusal: 'notReady',
    })
  })

  it('leaves a newer dialog’s registration alone when an older one unregisters', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    const older = answering('pressed')
    const newer = answering('pressed')
    const unregisterOlder = confirmer.registerDeleteConfirmer(older)
    confirmer.registerDeleteConfirmer(newer)
    unregisterOlder()

    await confirmer.confirmOpenDialog('delete-confirmation')

    expect(older).not.toHaveBeenCalled()
    expect(newer).toHaveBeenCalledOnce()
  })

  it('presses the Background button when asked, with the round-trip id, and says it pressed', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    const press = answering('pressed')
    confirmer.registerDeleteConfirmer(press)

    const verdict = await confirmer.confirmOpenDialog('delete-confirmation', undefined, {
      startInBackground: true,
      mcpRequestId: 'req-2',
    })

    expect(press).toHaveBeenCalledWith({ startInBackground: true, mcpRequestId: 'req-2' })
    expect(verdict).toEqual({ pressed: true })
  })

  it('carries the dialog’s typed refusal of a permanent delete in the background', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    confirmer.registerDeleteConfirmer(answering('refusedPermanentDelete'))

    const verdict = await confirmer.confirmOpenDialog('delete-confirmation', undefined, { startInBackground: true })

    expect(verdict).toEqual({ pressed: false, refusal: 'permanentDelete' })
  })

  it('says, typed, when the dialog took the press and started nothing', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    confirmer.registerDeleteConfirmer(answering('notStarted'))

    const verdict = await confirmer.confirmOpenDialog('delete-confirmation')

    expect(verdict).toEqual({ pressed: false, refusal: 'notStarted' })
  })

  it('presses nothing when the delete dialog is closed', async () => {
    const confirmer = createProgrammaticConfirm(makeDeps('none'))
    const press = answering('pressed')
    confirmer.registerDeleteConfirmer(press)

    await confirmer.confirmOpenDialog('delete-confirmation')

    expect(press).not.toHaveBeenCalled()
  })
})
