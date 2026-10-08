/**
 * Behavior tests for `MultiRenameDialog.svelte`: every error the backend can
 * answer gets its own words, Enter in a mask starts while Enter in the preset
 * name saves, a placeholder button inserts into the name mask, and presets save
 * and delete.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'

const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  applyMultiRename: vi.fn(),
  getMultiRenamePresets: vi.fn(),
  saveMultiRenamePreset: vi.fn(),
  deleteMultiRenamePreset: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  ...ipc,
}))

const READY = [{ row: 0, oldName: 'Ž.pdf', newName: 'Z.pdf', status: { type: 'ready' } }]

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

async function mountSheet(onApplied = vi.fn()): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MultiRenameDialog, {
    target,
    props: { target: { listingId: 'L', includeHidden: false, rows: null }, onApplied, onClose: vi.fn() },
  })
  await settle()
  return target
}

function inputs(root: HTMLElement): HTMLInputElement[] {
  return [...root.querySelectorAll<HTMLInputElement>('input:not([type="checkbox"])')]
}

function key(el: Element, k: string): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
}

describe('MultiRenameDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: READY })
    ipc.getMultiRenamePresets.mockResolvedValue([{ id: 'p1', name: 'Mine', spec: {} }])
    ipc.saveMultiRenamePreset.mockResolvedValue(undefined)
    ipc.deleteMultiRenamePreset.mockResolvedValue(undefined)
  })

  it.each([
    [{ type: 'spec', error: { type: 'badRegex', detail: 'x' } }],
    [{ type: 'spec', error: { type: 'nameMask', error: { type: 'unclosed', at: 1 } } }],
    [{ type: 'spec', error: { type: 'nameMask', error: { type: 'unknown', placeholder: 'Q' } } }],
    [{ type: 'gone' }],
    [{ type: 'notConnected', volumeId: 'v' }],
    [{ type: 'timedOut' }],
  ])('words a preview error (%o) instead of showing rows', async (error) => {
    ipc.previewMultiRename.mockResolvedValue({ ok: false, error })
    const root = await mountSheet()
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBeTruthy()
  })

  it.each([
    [{ type: 'nothingToRename' }],
    [{ type: 'previewOutOfDate' }],
    [{ type: 'readOnly' }],
    [{ type: 'couldntStart', reason: { type: 'busy' } }],
  ])('words a refused start (%o)', async (error) => {
    ipc.applyMultiRename.mockResolvedValue({ ok: false, error })
    const root = await mountSheet()
    key(inputs(root)[0], 'Enter')
    await settle()
    expect(ipc.applyMultiRename).toHaveBeenCalled()
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBeTruthy()
  })

  it('starts from Enter in the name mask and hands the operation up', async () => {
    ipc.applyMultiRename.mockResolvedValue({ ok: true, value: { operationId: 'op', renaming: 1 } })
    const onApplied = vi.fn()
    const root = await mountSheet(onApplied)
    key(inputs(root)[0], 'Enter')
    await settle()
    expect(onApplied).toHaveBeenCalledWith({ operationId: 'op', renaming: 1 })
  })

  it('inserts a placeholder into the name mask from its button', async () => {
    const root = await mountSheet()
    const counter = [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === '[C]')
    expect(counter).toBeTruthy()
    counter?.click()
    // The preview reruns after its debounce (`PREVIEW_DELAY_MS`).
    await vi.waitFor(() => {
      const lastSpec = ipc.previewMultiRename.mock.calls.at(-1)?.[3] as { nameMask: string } | undefined
      expect(lastSpec?.nameMask).toContain('[C]')
    })
  })

  it('saves a preset from Enter in its name, never starting a rename, and deletes it', async () => {
    const root = await mountSheet()
    const name = inputs(root).at(-1) as HTMLInputElement
    name.value = 'Mine'
    name.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    key(name, 'Enter')
    await settle()
    expect(ipc.saveMultiRenamePreset).toHaveBeenCalled()
    expect(ipc.applyMultiRename).not.toHaveBeenCalled()

    const del = [...root.querySelectorAll('button')].find((b) => !b.disabled && /delete/i.test(b.textContent))
    del?.click()
    await settle()
    expect(ipc.deleteMultiRenamePreset).toHaveBeenCalledWith('p1')
  })
})
