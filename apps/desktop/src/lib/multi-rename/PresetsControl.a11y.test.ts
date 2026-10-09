/**
 * Tier 3 a11y tests for `PresetsControl.svelte`: the Presets button with a preset
 * loaded, its menu open on numbered saved presets with a submenu showing, and the
 * save popover asking before it replaces a taken name.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import PresetsControl from './PresetsControl.svelte'
import { createMultiRenameState, type MultiRenameState } from './multi-rename-state.svelte'
import { DEFAULT_SPEC } from './spec'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  previewMultiRename: vi.fn(() =>
    Promise.resolve({ ok: true, value: { previewId: 1, counts: { ready: 0, unchanged: 0, problems: 0 }, rows: [] } }),
  ),
  getMultiRenamePreviewRows: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  applyMultiRename: vi.fn(),
  getMultiRenamePresets: vi.fn(() =>
    Promise.resolve([
      { id: 'p1', name: 'Bez diakritiky', spec: { ...DEFAULT_SPEC, removeDiacritics: true } },
      { id: 'p2', name: 'Photos', spec: { ...DEFAULT_SPEC, nameMask: 'IMG_[C]' } },
    ]),
  ),
  saveMultiRenamePreset: vi.fn(() => Promise.resolve()),
  deleteMultiRenamePreset: vi.fn(() => Promise.resolve()),
  renameMultiRenamePreset: vi.fn(() => Promise.resolve()),
  updateMultiRenamePreset: vi.fn(() => Promise.resolve()),
}))

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountControl(): Promise<{
  root: HTMLElement
  tool: MultiRenameState
  control: { openSave: () => void }
}> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const tool = createMultiRenameState('S')
  await tool.loadPresets()
  tool.loadPreset({ kind: 'saved', id: 'p2' })
  tool.update({ nameMask: 'IMG_[C]_x' })
  const control = mount(PresetsControl, { target: root, props: { tool } })
  cleanup = () => {
    void unmount(control)
    tool.dispose()
  }
  await settle()
  return { root, tool, control }
}

function key(k: string): void {
  ;(document.activeElement ?? document.body).dispatchEvent(
    new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }),
  )
}

describe('PresetsControl a11y', () => {
  it('the button, with an edited preset loaded, has no violations', async () => {
    const { root } = await mountControl()
    await expectNoA11yViolations(root)
  })

  it('the open menu, numbered rows and a submenu showing, has no violations', async () => {
    const { root } = await mountControl()
    root.querySelector('button')?.click()
    await settle()
    key('ArrowRight')
    await settle()
    if (!document.querySelector('[data-menu]')) throw new Error('the menu did not open')
    if (document.querySelectorAll('[role="menu"]').length < 2) throw new Error('the submenu did not open')
    await expectNoA11yViolations(document.body)
  })

  it('the save popover, asking before it replaces a taken name, has no violations', async () => {
    const { control } = await mountControl()
    control.openSave()
    await settle()
    const name = document.querySelector<HTMLInputElement>('.ui-popover input')
    if (!name) throw new Error('the save popover did not open')
    name.value = 'photos'
    name.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await settle()
    if (!document.querySelector('.ui-popover [role="alert"]')) throw new Error('no replace prompt')
    await expectNoA11yViolations(document.body)
  })
})
