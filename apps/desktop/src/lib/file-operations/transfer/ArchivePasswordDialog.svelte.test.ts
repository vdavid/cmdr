/**
 * Rendering + interaction tests for `ArchivePasswordDialog.svelte`.
 *
 * The dialog prompts for an encrypted archive's password before a copy/move out
 * of it can extract. Two shapes: the first prompt, and the re-prompt after a
 * rejected password (`wrongAttempt`), which shows distinct copy and starts with
 * an empty field. Tauri's dialog-tracking IPC is stubbed so it mounts in happy-dom.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import ArchivePasswordDialog from './ArchivePasswordDialog.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

function findButton(target: HTMLElement, text: string): HTMLButtonElement {
  const btn = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === text)
  if (!btn) throw new Error(`Button "${text}" not found`)
  return btn
}

async function settle(): Promise<void> {
  // onMount awaits a tick before focusing; give it a couple of turns.
  await tick()
  await tick()
}

let target: HTMLElement

beforeEach(() => {
  target = document.createElement('div')
  document.body.appendChild(target)
})

describe('ArchivePasswordDialog — first prompt', () => {
  it('renders the first-prompt copy, names the archive, and focuses the field', async () => {
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit: () => {}, onCancel: () => {} },
    })
    await settle()

    expect(target.querySelector('#archive-password-title')?.textContent).toBe('Password needed')
    expect(target.querySelector('#archive-password-message')?.textContent).toContain('photos.zip')
    expect(target.querySelector('#archive-password-message')?.textContent).toContain('password-protected')

    const input = target.querySelector('input[type="password"]') as HTMLInputElement
    expect(input).toBeTruthy()
    expect(document.activeElement).toBe(input)
    expect(input.value).toBe('')
  })

  it('submits the typed password on Unlock click', async () => {
    const onSubmit = vi.fn()
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit, onCancel: () => {} },
    })
    await settle()

    const input = target.querySelector('input[type="password"]') as HTMLInputElement
    input.value = 'hunter2'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    findButton(target, 'Unlock').click()
    expect(onSubmit).toHaveBeenCalledWith('hunter2')
  })

  it('submits on Enter in the field', async () => {
    const onSubmit = vi.fn()
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit, onCancel: () => {} },
    })
    await settle()

    const input = target.querySelector('input[type="password"]') as HTMLInputElement
    input.value = 'sesame'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onSubmit).toHaveBeenCalledWith('sesame')
  })

  it('disables Unlock (no submit) while the field is empty', async () => {
    const onSubmit = vi.fn()
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit, onCancel: () => {} },
    })
    await settle()

    expect(findButton(target, 'Unlock').disabled).toBe(true)
    // Enter with an empty field must not submit either.
    const input = target.querySelector('input[type="password"]') as HTMLInputElement
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('cancels on the Cancel button', async () => {
    const onCancel = vi.fn()
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: false, onSubmit: () => {}, onCancel },
    })
    await settle()

    findButton(target, 'Cancel').click()
    expect(onCancel).toHaveBeenCalledOnce()
  })
})

describe('ArchivePasswordDialog — wrong-attempt re-prompt', () => {
  it('shows distinct copy and starts with an empty field', async () => {
    mount(ArchivePasswordDialog, {
      target,
      props: { archiveName: 'photos.zip', wrongAttempt: true, onSubmit: () => {}, onCancel: () => {} },
    })
    await settle()

    expect(target.querySelector('#archive-password-title')?.textContent).toBe("That didn't work")
    const message = target.querySelector('#archive-password-message')?.textContent ?? ''
    expect(message).toContain("didn't unlock")
    expect(message).toContain('photos.zip')

    const input = target.querySelector('input[type="password"]') as HTMLInputElement
    expect(input.value).toBe('')
  })
})
