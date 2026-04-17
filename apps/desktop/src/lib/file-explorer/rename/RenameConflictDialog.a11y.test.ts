/**
 * Tier 3 a11y tests for `RenameConflictDialog.svelte`.
 *
 * `alertdialog` role with a side-by-side file comparison and four action
 * buttons. Tests cover the "renamed is newer", "existing is newer", and
 * "same mtime, different size" cases.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import RenameConflictDialog from './RenameConflictDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatDateTime: vi.fn((d: number | undefined) => (d ? '2025-03-14 10:30' : '')),
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
}))

describe('RenameConflictDialog a11y', () => {
  it('renamed is newer has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'report.md', size: 2048, modifiedAt: 1710000000000 },
        existingFile: { name: 'report.md', size: 1024, modifiedAt: 1700000000000 },
        onResolve: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('existing is newer has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'draft.txt', size: 5000, modifiedAt: 1700000000000 },
        existingFile: { name: 'draft.txt', size: 5200, modifiedAt: 1710000000000 },
        onResolve: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('without mtimes has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RenameConflictDialog, {
      target,
      props: {
        renamedFile: { name: 'notes.txt', size: 1024, modifiedAt: undefined },
        existingFile: { name: 'notes.txt', size: 2048, modifiedAt: undefined },
        onResolve: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
