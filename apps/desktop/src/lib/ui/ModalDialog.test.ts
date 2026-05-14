/**
 * Behavior tests for `ModalDialog.svelte`. Tier-3 a11y wiring lives in
 * `ModalDialog.a11y.test.ts`. This file covers focus restoration on close.
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
