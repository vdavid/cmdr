/**
 * Behavior tests for `ModalDialog.svelte`. Tier-3 a11y wiring lives in
 * `ModalDialog.a11y.test.ts`. This file covers focus restoration on close
 * and the Enter-on-focused-button suppression.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, unmount, tick, createRawSnippet } from 'svelte'
import ModalDialog from './ModalDialog.svelte'

// Avoid Tauri IPC side-effects from notifyDialogOpened / notifyDialogClosed.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const titleSnippet = createRawSnippet(() => ({ render: () => `<span>Dialog title</span>` }))
const bodySnippet = createRawSnippet(() => ({ render: () => `<p>Body.</p>` }))
const footerSnippet = createRawSnippet(() => ({ render: () => `<button>OK</button>` }))

describe('ModalDialog focus restoration', () => {
  it('restores focus to the previously focused element on destroy', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()
    expect(document.activeElement).toBe(trigger)

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet },
    })

    // Let onMount run so the dialog captures `trigger` as previously focused.
    await tick()

    // Simulate "dialog has stolen focus".
    const otherEl = document.createElement('input')
    document.body.appendChild(otherEl)
    otherEl.focus()
    expect(document.activeElement).toBe(otherEl)

    void unmount(component)
    await tick()

    expect(document.activeElement).toBe(trigger)

    otherEl.remove()
    trigger.remove()
    target.remove()
  })

  it('does not throw if the previously focused element is no longer in the DOM', async () => {
    const trigger = document.createElement('button')
    document.body.appendChild(trigger)
    trigger.focus()

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet },
    })
    await tick()

    trigger.remove()
    expect(() => {
      void unmount(component)
    }).not.toThrow()
    await tick()

    target.remove()
  })
})

describe('ModalDialog body padding and resizing', () => {
  function mountDialog(props: Record<string, unknown>) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodySnippet, ...props },
    })
    return target
  }

  it('wraps children in a .modal-body element', () => {
    const target = mountDialog({})
    const body = target.querySelector('.modal-body')
    expect(body).not.toBeNull()
    expect(body?.textContent).toContain('Body.')
    target.remove()
  })

  it('adds .no-footer when there is no footer so the body owns bottom padding', () => {
    const target = mountDialog({})
    expect(target.querySelector('.modal-body')?.classList.contains('no-footer')).toBe(true)
    target.remove()
  })

  it('drops .no-footer when a footer is present (footer owns bottom padding)', () => {
    const target = mountDialog({ footer: footerSnippet })
    expect(target.querySelector('.modal-body')?.classList.contains('no-footer')).toBe(false)
    target.remove()
  })

  it('adds .flush when padded is false (full-bleed body)', () => {
    const target = mountDialog({ padded: false })
    expect(target.querySelector('.modal-body')?.classList.contains('flush')).toBe(true)
    target.remove()
  })

  it('does not add .flush by default (padded defaults to true)', () => {
    const target = mountDialog({})
    expect(target.querySelector('.modal-body')?.classList.contains('flush')).toBe(false)
    target.remove()
  })

  it('adds .resizable to the dialog when resizable is true', () => {
    const target = mountDialog({ resizable: true })
    expect(target.querySelector('.modal-dialog')?.classList.contains('resizable')).toBe(true)
    target.remove()
  })

  it('does not add .resizable by default', () => {
    const target = mountDialog({})
    expect(target.querySelector('.modal-dialog')?.classList.contains('resizable')).toBe(false)
    target.remove()
  })
})

describe('ModalDialog Enter key', () => {
  // Body containing both a button (Cancel) and an input. The test dispatches Enter
  // from each and verifies the dialog's default-action handler only fires for the input.
  const bodyWithControls = createRawSnippet(() => ({
    render: () => `<div><button id="cancel-btn">Cancel</button><input id="path-input" /></div>`,
  }))

  it('suppresses the default action when Enter is pressed on a focused button', async () => {
    const onkeydown = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodyWithControls, onkeydown },
    })
    await tick()

    const cancelBtn = target.querySelector<HTMLButtonElement>('#cancel-btn')
    if (!cancelBtn) throw new Error('cancel button not rendered')
    cancelBtn.focus()
    cancelBtn.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))

    expect(onkeydown).not.toHaveBeenCalled()
    target.remove()
  })

  it('still fires the default action when Enter is pressed on a non-button element', async () => {
    const onkeydown = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ModalDialog, {
      target,
      props: { titleId: 't', title: titleSnippet, children: bodyWithControls, onkeydown },
    })
    await tick()

    const input = target.querySelector<HTMLInputElement>('#path-input')
    if (!input) throw new Error('input not rendered')
    input.focus()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))

    expect(onkeydown).toHaveBeenCalledTimes(1)
    target.remove()
  })
})
