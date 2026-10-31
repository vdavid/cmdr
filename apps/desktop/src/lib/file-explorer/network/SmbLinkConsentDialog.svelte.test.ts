/**
 * The one place a person allows a sign-in on another server through an SMB link:
 * it names both servers, only a deliberate press allows it, and anything else
 * leaves the backend untouched.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'

// eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `allowSmbLinkTarget(server, target)`, whose shape the test exists to pin
const allowSmbLinkTarget = vi.fn<(server: string, target: string) => Promise<boolean>>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  allowSmbLinkTarget: (server: string, target: string) => allowSmbLinkTarget(server, target),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import SmbLinkConsentDialog from './SmbLinkConsentDialog.svelte'

function open(onAllowed: () => void, onCancel: () => void) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SmbLinkConsentDialog, {
    target,
    props: { server: 'nas.local', target: 'fs01.example.com', onAllowed, onCancel },
  })
  flushSync()
  return { target, close: () => void unmount(component) }
}

function buttonNamed(root: HTMLElement, name: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((button) => button.textContent.includes(name))
  if (!found) throw new Error(`no button named ${name}`)
  return found
}

describe('SmbLinkConsentDialog', () => {
  beforeEach(() => {
    allowSmbLinkTarget.mockReset()
    allowSmbLinkTarget.mockResolvedValue(true)
  })

  it('names both servers plainly', () => {
    const { target, close } = open(vi.fn(), vi.fn())
    expect(target.textContent).toContain('nas.local')
    expect(target.textContent).toContain('fs01.example.com')
    close()
  })

  it('allows the pair on the press, then lets the pane list again', async () => {
    const onAllowed = vi.fn()
    const { target, close } = open(onAllowed, vi.fn())
    buttonNamed(target, 'Allow and sign in').click()
    await tick()
    await vi.waitFor(() => {
      expect(onAllowed).toHaveBeenCalledOnce()
    })
    expect(allowSmbLinkTarget).toHaveBeenCalledWith('nas.local', 'fs01.example.com')
    close()
  })

  it('allows nothing on Not now', () => {
    const onCancel = vi.fn()
    const { target, close } = open(vi.fn(), onCancel)
    buttonNamed(target, 'Not now').click()
    expect(onCancel).toHaveBeenCalledOnce()
    expect(allowSmbLinkTarget).not.toHaveBeenCalled()
    close()
  })

  it('allows nothing on a stray Enter: this hands a password to another machine', () => {
    const { target, close } = open(vi.fn(), vi.fn())
    target
      .querySelector('[role="alertdialog"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(allowSmbLinkTarget).not.toHaveBeenCalled()
    close()
  })

  it('stays closed-to-allow when the backend refuses the name', async () => {
    allowSmbLinkTarget.mockResolvedValue(false)
    const onAllowed = vi.fn()
    const onCancel = vi.fn()
    const { target, close } = open(onAllowed, onCancel)
    buttonNamed(target, 'Allow and sign in').click()
    await vi.waitFor(() => {
      expect(onCancel).toHaveBeenCalledOnce()
    })
    expect(onAllowed).not.toHaveBeenCalled()
    close()
  })
})
