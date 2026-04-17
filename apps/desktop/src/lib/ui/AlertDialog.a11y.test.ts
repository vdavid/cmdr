/**
 * Tier 3 a11y tests for `AlertDialog.svelte`.
 *
 * Alert dialogs must use role="alertdialog" with labelled title + described
 * message and a primary action button. These tests check all of that via
 * axe-core — text-only variants (short message, long message, custom button
 * label) are covered.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import AlertDialog from './AlertDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

describe('AlertDialog a11y', () => {
  it('default (single OK button) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AlertDialog, {
      target,
      props: {
        title: 'Something went wrong',
        message: 'We could not complete your request.',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('custom button label has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AlertDialog, {
      target,
      props: {
        title: 'Heads up',
        message: 'You have unsaved changes.',
        buttonText: 'Dismiss',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long message with multiple sentences has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AlertDialog, {
      target,
      props: {
        title: 'Read error',
        message:
          'We could not read the file. It may have been moved or deleted since you opened the folder. Try refreshing the pane.',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('terse title + message has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AlertDialog, {
      target,
      props: {
        title: 'Error',
        message: 'Not found.',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
