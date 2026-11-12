/**
 * Tier 3 a11y tests for `DeleteAllDataDialog.svelte`, plus what the dialog owns: Cancel is the
 * default, nothing can close or re-fire it mid-delete, closing the outcome quits, and each
 * summary gets the right "What goes" lines.
 */

import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { mount, tick } from 'svelte'
import { computeAccessibleDescription } from 'dom-accessibility-api'
import DeleteAllDataDialog from './DeleteAllDataDialog.svelte'
import type { DataWipeView } from './delete-all-data'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const SUMMARY = { totalBytes: 1_234_000_000, modelBytes: 4_100_000_000, hasLicense: true }

const VIEWS: Record<string, DataWipeView> = {
  confirm: { kind: 'confirm', summary: SUMMARY, refusal: null },
  refused: { kind: 'confirm', summary: SUMMARY, refusal: { type: 'fileOperationsRunning', count: 1 } },
  deleting: { kind: 'deleting', summary: SUMMARY },
  done: { kind: 'done', report: { leftovers: [] } },
  partial: {
    kind: 'done',
    report: {
      leftovers: [
        { type: 'path', path: '/Users/someone/Library/Logs/com.veszelovszki.cmdr' },
        { type: 'secretItems', count: 2, itemName: 'Cmdr' },
      ],
    },
  },
}

async function mountDialog(view: DataWipeView) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onConfirm = vi.fn()
  const onCancel = vi.fn()
  const onQuit = vi.fn()
  mount(DeleteAllDataDialog, { target, props: { view, onConfirm, onCancel, onQuit } })
  await tick()
  return { target, onConfirm, onCancel, onQuit }
}

function buttons(target: HTMLElement): HTMLButtonElement[] {
  return [...target.querySelectorAll<HTMLButtonElement>('.modal-footer-actions button')]
}

function escape(target: HTMLElement): void {
  target
    .querySelector('[role="alertdialog"]')
    ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
}

describe('DeleteAllDataDialog a11y', () => {
  for (const [name, view] of Object.entries(VIEWS)) {
    it(`${name} has no a11y violations`, async () => {
      const { target } = await mountDialog(view)
      await expectNoA11yViolations(target)
      target.remove()
    })
  }
})

describe('DeleteAllDataDialog behaviour', () => {
  it('is an alertdialog that reads its intro as the description', async () => {
    const { target } = await mountDialog(VIEWS.confirm)
    const dialog = target.querySelector('[role="alertdialog"]')
    expect(dialog).not.toBeNull()
    const message = target.querySelector('.message')?.textContent.trim() ?? ''
    expect(message).toContain('can’t be undone')
    expect(computeAccessibleDescription(dialog as Element)).toBe(message)
    target.remove()
  })

  it('lists the license with how to get it back, and the models with their size', async () => {
    const { target } = await mountDialog(VIEWS.confirm)
    const items = [...target.querySelectorAll('.goes li')].map((li) => li.textContent.trim())
    expect(items.some((i) => i.includes('purchase email'))).toBe(true)
    expect(items.some((i) => i.startsWith('Downloaded AI models (') && i.endsWith(')'))).toBe(true)
    target.remove()
  })

  it('leaves the license and models out when there are none', async () => {
    const { target } = await mountDialog({
      kind: 'confirm',
      summary: { totalBytes: 10, modelBytes: 0, hasLicense: false },
      refusal: null,
    })
    const text = target.querySelector('.goes')?.textContent ?? ''
    expect(text).not.toContain('license')
    expect(text).not.toContain('AI models')
    target.remove()
  })

  it('says what stays', async () => {
    const { target } = await mountDialog(VIEWS.confirm)
    expect(target.querySelector('.stays')?.textContent).toContain('Your own files')
    target.remove()
  })

  it('focuses Cancel, so Enter walks away', async () => {
    const { target } = await mountDialog(VIEWS.confirm)
    await new Promise((resolve) => requestAnimationFrame(resolve))
    expect(document.activeElement?.textContent.trim()).toBe('Cancel')
    target.remove()
  })

  it('Escape cancels on the confirm view', async () => {
    const { target, onCancel, onConfirm } = await mountDialog(VIEWS.confirm)
    escape(target)
    expect(onCancel).toHaveBeenCalledTimes(1)
    expect(onConfirm).not.toHaveBeenCalled()
    target.remove()
  })

  it('mid-delete, both buttons are dead and Escape does nothing', async () => {
    const { target, onCancel } = await mountDialog(VIEWS.deleting)
    expect(buttons(target).every((b) => b.disabled)).toBe(true)
    escape(target)
    expect(onCancel).not.toHaveBeenCalled()
    target.remove()
  })

  it('shows why a confirm was refused', async () => {
    const { target } = await mountDialog(VIEWS.refused)
    expect(target.querySelector('[role="alert"]')?.textContent).toContain('still running')
    target.remove()
  })

  it('after the delete, offers only Quit, and Escape quits too', async () => {
    const { target, onQuit } = await mountDialog(VIEWS.done)
    expect(buttons(target).map((b) => b.textContent.trim())).toEqual(['Quit Cmdr'])
    escape(target)
    expect(onQuit).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it.each([
    ['macos', 'Delete all and quit', 'Quit Cmdr', 'this Mac', 'Keychain Access'],
    ['windows', 'Delete all and exit', 'Exit Cmdr', 'this PC', 'Credential Manager'],
    ['linux', 'Delete all and quit', 'Quit Cmdr', 'this computer', 'your keyring'],
  ] as const)(
    'speaks %s’s words: the confirm and quit buttons, the machine, and where the secrets live',
    async (platform, confirmLabel, quitLabel, machine, secretStore) => {
      onTestFinished(withPlatform(platform))
      const confirm = await mountDialog(VIEWS.confirm)
      expect(buttons(confirm.target).map((b) => b.textContent.trim())).toContain(confirmLabel)
      expect(confirm.target.textContent).toContain(machine)
      confirm.target.remove()

      const partial = await mountDialog(VIEWS.partial)
      expect(buttons(partial.target).map((b) => b.textContent.trim())).toEqual([quitLabel])
      expect(partial.target.querySelector('.leftovers')?.textContent).toContain(secretStore)
      partial.target.remove()
    },
  )

  it('names every leftover: the path verbatim, and the keychain items by name', async () => {
    const { target } = await mountDialog(VIEWS.partial)
    const lines = [...target.querySelectorAll('.leftovers li')].map((li) => li.textContent.trim())
    expect(lines[0]).toBe('/Users/someone/Library/Logs/com.veszelovszki.cmdr')
    expect(lines[1]).toContain('“Cmdr”')
    target.remove()
  })
})
