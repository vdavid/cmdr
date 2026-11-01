/**
 * The rename editor as an observable state: seen by the start gate and the backend's tracker,
 * closable by id, and gone once nothing holds it open.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { notifyDialogOpened, notifyDialogClosed } = vi.hoisted(() => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ notifyDialogOpened, notifyDialogClosed }))

import { closeDialogById } from '#lib/ui/dialog-close-registry.js'
import { _resetOpenDialogsForTesting, blockingSoftDialog, isDialogOpen } from '#lib/ui/open-dialogs.svelte.js'
import { createRenameState, type RenameTarget } from './rename-state.svelte'

const target: RenameTarget = {
  path: '/x/a.txt',
  originalName: 'a.txt',
  parentPath: '/x',
  isDirectory: false,
  nameKind: 'file',
}

beforeEach(() => {
  _resetOpenDialogsForTesting()
  vi.clearAllMocks()
})

describe('the rename editor as a state', () => {
  it('blocks a start while open, tells the backend, and clears both when it ends', () => {
    const state = createRenameState()

    state.activate(target)
    expect(blockingSoftDialog()).toBe('rename')
    expect(notifyDialogOpened).toHaveBeenCalledWith('rename')

    state.cancel()
    expect(blockingSoftDialog()).toBeNull()
    expect(notifyDialogClosed).toHaveBeenCalledWith('rename')
  })

  it('clears in the same tick, so a Move dialog waiting on it opens past the gate', () => {
    const state = createRenameState()
    state.activate(target)
    state.cancel()
    expect(isDialogOpen('rename')).toBe(false)
  })

  it('says it once across a chained step, which reactivates with the editor still up', () => {
    const state = createRenameState()
    state.activate(target)
    state.activate({ ...target, path: '/x/b.txt', originalName: 'b.txt' })
    expect(notifyDialogOpened).toHaveBeenCalledTimes(1)
    expect(notifyDialogClosed).not.toHaveBeenCalled()
    state.cancel()
  })

  it('stays open until the last editor ends, with one on each pane', () => {
    const left = createRenameState()
    const right = createRenameState()
    left.activate(target)
    right.activate(target)

    left.cancel()
    expect(isDialogOpen('rename')).toBe(true)
    right.cancel()
    expect(isDialogOpen('rename')).toBe(false)
    expect(notifyDialogClosed).toHaveBeenCalledTimes(1)
  })

  it('goes with its pane, or file operations stay blocked for the session', () => {
    const state = createRenameState()
    state.activate(target)
    state.dispose()
    expect(blockingSoftDialog()).toBeNull()
    state.dispose()
    expect(notifyDialogClosed).toHaveBeenCalledTimes(1)
  })

  it('closes by id through the dismiss it was given, and only while open', () => {
    const dismiss = vi.fn()
    const state = createRenameState(dismiss)
    expect(closeDialogById('rename')).toBe(false)

    state.activate(target)
    expect(closeDialogById('rename')).toBe(true)
    expect(dismiss).toHaveBeenCalledTimes(1)

    state.cancel()
    expect(closeDialogById('rename')).toBe(false)
  })
})
