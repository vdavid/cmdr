/**
 * Tier 3 a11y tests for the rename surfaces: the extension-change dialog, the
 * inline editor, and the conflict dialog.
 *
 * One file per component would cost about three times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * No stub here disagrees between blocks: both dialogs wanted the same
 * `$lib/tauri-commands` pair, and the other two stubs have one consumer each. Every
 * stub spreads the real module first, so the inline editor, which never stubbed
 * anything, still sees every un-stubbed export.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import ExtensionChangeDialog from './ExtensionChangeDialog.svelte'
import InlineRenameEditor from './InlineRenameEditor.svelte'
import RenameConflictDialog from './RenameConflictDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  setSetting: vi.fn(),
}))

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getFileSizeFormat: () => 'binary',
  formatDateTime: vi.fn((d: number | undefined) => (d ? '2025-03-14 10:30' : '')),
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  formattedDate: vi.fn((d: number | undefined) =>
    d
      ? {
          text: '2025-03-14 10:30',
          segments: [
            { text: '2025', ageClass: 'age-fresh' as const },
            { text: '-', ageClass: null },
            { text: '03', ageClass: null },
            { text: '-', ageClass: null },
            { text: '14', ageClass: null },
            { text: ' ', ageClass: null },
            { text: '10', ageClass: null },
            { text: ':', ageClass: null },
            { text: '30', ageClass: null },
          ],
        }
      : { text: '', segments: [] },
  ),
}))

// The dialogs portal into `document.body` and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `ExtensionChangeDialog.svelte`.
 *
 * Simple confirmation dialog with a description, "Always allow"
 * checkbox, and two action buttons.
 */
describe('ExtensionChangeDialog a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExtensionChangeDialog, {
      target,
      props: {
        oldExtension: 'txt',
        newExtension: 'md',
        onKeepOld: () => {},
        onUseNew: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('uncommon extension switch has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExtensionChangeDialog, {
      target,
      props: {
        oldExtension: 'png',
        newExtension: 'jpg',
        onKeepOld: () => {},
        onUseNew: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `InlineRenameEditor.svelte`.
 *
 * Input field with aria-live validation region. Tests cover ok, error,
 * and warning severities, plus the shake-animation state.
 */
describe('InlineRenameEditor a11y', () => {
  const noop = () => {}

  it('default (severity=ok) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: 'report.md',
        severity: 'ok',
        shaking: false,
        ariaLabel: 'Rename file',
        sessionId: 1,
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onClickAway: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('error severity with validation message has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: 'bad/name.txt',
        severity: 'error',
        shaking: false,
        ariaLabel: 'Rename file',
        sessionId: 1,
        ariaInvalid: true,
        validationMessage: 'Slashes are not allowed in file names',
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onClickAway: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('warning severity has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: 'archive.tar',
        severity: 'warning',
        shaking: false,
        ariaLabel: 'Rename file',
        sessionId: 1,
        validationMessage: 'Extension changed from .zip to .tar',
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onClickAway: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('shaking state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(InlineRenameEditor, {
      target,
      props: {
        value: '',
        severity: 'error',
        shaking: true,
        ariaLabel: 'Rename file',
        sessionId: 1,
        ariaInvalid: true,
        onInput: noop,
        onSubmit: noop,
        onCancel: noop,
        onClickAway: noop,
        onShakeEnd: noop,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `RenameConflictDialog.svelte`.
 *
 * `alertdialog` role with a side-by-side file comparison and four action
 * buttons. Tests cover the "renamed is newer", "existing is newer", and
 * "same mtime, different size" cases.
 */
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
