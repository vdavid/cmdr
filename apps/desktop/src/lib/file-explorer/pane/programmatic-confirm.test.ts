import { describe, expect, it, vi } from 'vitest'
import { createProgrammaticConfirm, type ProgrammaticConfirmDeps } from './programmatic-confirm'

function makeDeps(open: 'transfer' | 'delete' | 'none'): ProgrammaticConfirmDeps {
  return {
    isTransferDialogOpen: () => open === 'transfer',
    isDeleteDialogOpen: () => open === 'delete',
    isArchivePasswordOpen: () => false,
    supplyStoredPassword: vi.fn(),
  }
}

describe('confirmOpenDialog on the transfer dialog, in the background', () => {
  it('presses the Background button under the named policy', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('transfer'))
    const press = vi.fn()
    confirmer.registerTransferConfirmer(press)

    const verdict = confirmer.confirmOpenDialog('transfer-confirmation', 'overwrite_all', { startInBackground: true })

    expect(press).toHaveBeenCalledWith('overwrite', { startInBackground: true })
    expect(verdict).toEqual({ pressed: true })
  })
})

describe('confirmOpenDialog on the delete dialog', () => {
  it('presses the mounted dialog’s own confirm', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    const press = vi.fn()
    confirmer.registerDeleteConfirmer(press)

    confirmer.confirmOpenDialog('delete-confirmation')

    expect(press).toHaveBeenCalledOnce()
  })

  it('confirms nothing while the dialog is open but not mounted yet', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))

    expect(() => {
      confirmer.confirmOpenDialog('delete-confirmation')
    }).not.toThrow()
  })

  it('leaves a newer dialog’s registration alone when an older one unregisters', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    const older = vi.fn()
    const newer = vi.fn()
    const unregisterOlder = confirmer.registerDeleteConfirmer(older)
    confirmer.registerDeleteConfirmer(newer)
    unregisterOlder()

    confirmer.confirmOpenDialog('delete-confirmation')

    expect(older).not.toHaveBeenCalled()
    expect(newer).toHaveBeenCalledOnce()
  })

  it('presses the Background button when asked, and says it pressed', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    const press = vi.fn(() => 'pressed' as const)
    confirmer.registerDeleteConfirmer(press)

    const verdict = confirmer.confirmOpenDialog('delete-confirmation', undefined, { startInBackground: true })

    expect(press).toHaveBeenCalledWith({ startInBackground: true })
    expect(verdict).toEqual({ pressed: true })
  })

  it('carries the dialog’s typed refusal of a permanent delete in the background', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))
    confirmer.registerDeleteConfirmer(() => 'refusedPermanentDelete')

    const verdict = confirmer.confirmOpenDialog('delete-confirmation', undefined, { startInBackground: true })

    expect(verdict).toEqual({ pressed: false, refusal: 'permanentDelete' })
  })

  it('says nothing was pressed when the dialog is open but not mounted yet', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('delete'))

    expect(confirmer.confirmOpenDialog('delete-confirmation', undefined, { startInBackground: true })).toEqual({
      pressed: false,
      refusal: 'notReady',
    })
  })

  it('presses nothing when the delete dialog is closed', () => {
    const confirmer = createProgrammaticConfirm(makeDeps('none'))
    const press = vi.fn()
    confirmer.registerDeleteConfirmer(press)

    confirmer.confirmOpenDialog('delete-confirmation')

    expect(press).not.toHaveBeenCalled()
  })
})
