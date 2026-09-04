/**
 * `AlertDialog`'s layering prop. The rest of the primitive (widths, the path
 * box, the Enter handler) is exercised through its callers and through
 * `overlays.a11y.test.ts`; what's pinned here is the one guarantee a caller
 * can't verify for itself.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { ComponentProps } from 'svelte'

// Avoid Tauri IPC side-effects from notifyDialogOpened / notifyDialogClosed.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import AlertDialog from './AlertDialog.svelte'

type Overrides = Partial<ComponentProps<typeof AlertDialog>>

async function render(props: Overrides = {}): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AlertDialog, {
    target,
    props: { title: 'Cmdr on macOS 11', message: 'Best effort here.', onClose: () => {}, ...props },
  })
  await tick()
  return target
}

describe('AlertDialog layering', () => {
  it('stays on the shared modal layer by default', async () => {
    const target = await render()
    expect(target.querySelector('.modal-overlay')?.classList.contains('topmost')).toBe(false)
  })

  it('forwards `topmost` to the overlay, so an app-raised alert lands over an open dialog', async () => {
    const target = await render({ topmost: true })
    expect(target.querySelector('.modal-overlay')?.classList.contains('topmost')).toBe(true)
  })
})
