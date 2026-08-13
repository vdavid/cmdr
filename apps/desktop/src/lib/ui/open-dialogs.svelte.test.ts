/**
 * The open-dialog set and the gate policy it reads.
 *
 * The set's exhaustiveness is a COMPILE-time property, not something a test can
 * assert: `SOFT_DIALOG_REGISTRY` requires a `whileOpen` verdict per entry, so a new
 * dialog fails to build until its author answers whether an operation may start
 * behind it. What's left to pin here is the runtime behaviour: which of several
 * open dialogs gets named, and that an opt-out actually carries its reason.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import {
  markDialogOpen,
  markDialogClosed,
  blockingSoftDialog,
  isAnySoftDialogOpen,
  _resetOpenDialogsForTesting,
} from './open-dialogs.svelte'
import { SOFT_DIALOG_REGISTRY, dialogBlocksOperations } from './dialog-registry'

beforeEach(() => {
  _resetOpenDialogsForTesting()
})

describe('the open-dialog set', () => {
  it('is empty until a dialog mounts', () => {
    expect(isAnySoftDialogOpen()).toBe(false)
    expect(blockingSoftDialog()).toBeNull()
  })

  it('names the dialog standing in the way', () => {
    markDialogOpen('transfer-progress')

    expect(blockingSoftDialog()).toBe('transfer-progress')
  })

  it('names the TOPMOST one when dialogs stack', () => {
    // A rollback confirmation over the progress dialog: closing the one underneath
    // isn't what an agent (or a person) has to do first.
    markDialogOpen('transfer-progress')
    markDialogOpen('rollback-confirmation')

    expect(blockingSoftDialog()).toBe('rollback-confirmation')
  })

  it('forgets a dialog that closed', () => {
    markDialogOpen('search')
    markDialogClosed('search')

    expect(isAnySoftDialogOpen()).toBe(false)
    expect(blockingSoftDialog()).toBeNull()
  })

  it('looks past a dialog that lets operations through', () => {
    markDialogOpen('viewer-copy-confirm')

    expect(isAnySoftDialogOpen()).toBe(true)
    expect(blockingSoftDialog()).toBeNull()
  })
})

describe('the gate policy', () => {
  it('blocks while search is up', () => {
    // Search is a full dialog over the panes, so a copy started behind it would
    // stack a confirmation on top of what the user is reading.
    expect(dialogBlocksOperations('search')).toBe(true)
  })

  it('lets the viewer window through', () => {
    expect(dialogBlocksOperations('viewer-copy-confirm')).toBe(false)
    expect(dialogBlocksOperations('viewer-copy-refuse')).toBe(false)
  })

  it('makes every opt-out say why', () => {
    const optOuts = SOFT_DIALOG_REGISTRY.filter((d) => !d.whileOpen.blocks)

    expect(optOuts.length).toBeGreaterThan(0)
    for (const dialog of optOuts) {
      expect(dialog.whileOpen.blocks).toBe(false)
      if (!dialog.whileOpen.blocks) expect(dialog.whileOpen.reason.length).toBeGreaterThan(20)
    }
  })
})
