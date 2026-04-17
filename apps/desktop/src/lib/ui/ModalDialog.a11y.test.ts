/**
 * Tier 3 a11y tests for `ModalDialog.svelte`.
 *
 * ModalDialog is the base for every dialog in the app. These tests cover
 * ARIA wiring (role, aria-modal, aria-labelledby, aria-describedby) and
 * the close-button label. Focus-trap and Escape behavior are covered in
 * the E2E tier — jsdom's focus model is incomplete.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import ModalDialog from './ModalDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Avoid Tauri IPC side-effects from notifyDialogOpened / notifyDialogClosed.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const titleSnippet = createRawSnippet(() => ({ render: () => `<span>Dialog title</span>` }))
const bodySnippet = createRawSnippet(() => ({
  render: () => `<div><p>Dialog body copy explaining the action.</p></div>`,
}))

describe('ModalDialog a11y', () => {
  it('renders without violations with title + children', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations when onclose adds the close button', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        onclose: () => {},
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations with role="alertdialog"', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        role: 'alertdialog',
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations with aria-describedby wired to body', async () => {
    const descBody = createRawSnippet(() => ({
      render: () => `<div id="test-dialog-desc">Extra description for the dialog.</div>`,
    }))
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        ariaDescribedby: 'test-dialog-desc',
        title: titleSnippet,
        children: descBody,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations with blur overlay and draggable=false', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        blur: true,
        draggable: false,
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
