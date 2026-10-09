/**
 * Behavior tests for `MultiRenameDialog.svelte`: every error the backend can
 * answer gets its own words, Enter in a mask starts, a placeholder button inserts
 * into the name mask, and the presets: F2 opens their menu, a digit loads one,
 * the button says when the fields drifted from it, and ⌘S saves (asking before it
 * replaces), with Enter and Escape staying inside the name popover.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import { runMenuClaim } from '$lib/commands/menu-claims'

const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  getMultiRenamePreviewRows: vi.fn(),
  applyMultiRename: vi.fn(),
  getMultiRenamePresets: vi.fn(),
  saveMultiRenamePreset: vi.fn(),
  deleteMultiRenamePreset: vi.fn(),
  renameMultiRenamePreset: vi.fn(),
  updateMultiRenamePreset: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  ...ipc,
}))

const READY = {
  previewId: 1,
  counts: { ready: 1, unchanged: 0, problems: 0 },
  rows: [
    { row: 0, oldName: 'Ž.pdf', newName: 'Z.pdf', status: { type: 'ready' }, iconId: 'ext:pdf', isDirectory: false },
  ],
}

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

async function mountSheet(onApplied = vi.fn(), onClose = vi.fn()): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MultiRenameDialog, {
    target,
    props: { session: { sessionId: 'S', count: 1 }, onApplied, onClose },
  })
  await settle()
  return target
}

function inputs(root: HTMLElement): HTMLInputElement[] {
  return [...root.querySelectorAll<HTMLInputElement>('input:not([type="checkbox"])')]
}

function key(el: Element, k: string, init: KeyboardEventInit = {}): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init }))
}

function presetsButton(root: HTMLElement): HTMLButtonElement {
  const button = root.querySelector<HTMLButtonElement>('.presets button')
  if (!button) throw new Error('no Presets button')
  return button
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
    [{ type: 'sessionClosed' }],
    [{ type: 'notConnected', volumeId: 'v' }],
    [{ type: 'timedOut' }],
  ])('words a preview error (%o) instead of showing rows', async (error) => {
    ipc.previewMultiRename.mockResolvedValue({ ok: false, error })
    const root = await mountSheet()
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBeTruthy()
  })

  it('says a preview that ran out of time took too long, not that renaming couldn’t start', async () => {
    const alertFor = async (error: unknown): Promise<string | undefined> => {
      ipc.previewMultiRename.mockResolvedValue({ ok: false, error })
      const root = await mountSheet()
      return root.querySelector('[role="alert"]')?.textContent.trim()
    }
    const timedOut = await alertFor({ type: 'timedOut' })
    const internal = await alertFor({ type: 'internal', detail: 'x' })
    expect(timedOut).toBeTruthy()
    expect(timedOut).not.toBe(internal)
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
    const started = { operationId: 'op', renaming: 1, swapsLeftOut: 0 }
    ipc.applyMultiRename.mockResolvedValue({ ok: true, value: started })
    const onApplied = vi.fn()
    const root = await mountSheet(onApplied)
    key(inputs(root)[0], 'Enter')
    await settle()
    expect(ipc.applyMultiRename).toHaveBeenCalledWith('S', 1)
    expect(onApplied).toHaveBeenCalledWith(started)
  })

  describe('preview list', () => {
    const MIXED = {
      previewId: 3,
      counts: { ready: 1, unchanged: 0, problems: 2 },
      rows: [
        {
          row: 0,
          oldName: 'a.pdf',
          newName: 'b.pdf',
          status: { type: 'ready' },
          iconId: 'ext:pdf',
          isDirectory: false,
        },
        {
          row: 1,
          oldName: 'c.pdf',
          newName: 'd.pdf',
          status: { type: 'targetExists' },
          iconId: 'ext:pdf',
          isDirectory: false,
        },
        {
          row: 2,
          oldName: 'gone.pdf',
          newName: 'gone.pdf',
          status: { type: 'missing' },
          iconId: null,
          isDirectory: false,
        },
      ],
    }

    function cellTexts(root: HTMLElement, column: string): string[] {
      return [...root.querySelectorAll(`[role="cell"][data-column="${column}"]`)].map((c) => c.textContent.trim())
    }

    it('lists each file’s old and new name as table rows', async () => {
      ipc.previewMultiRename.mockResolvedValue({ ok: true, value: MIXED })
      const root = await mountSheet()
      expect(root.querySelector('[role="table"]')?.getAttribute('aria-label')).toBe('Preview')
      expect(cellTexts(root, 'old-name')).toEqual(['a.pdf', 'c.pdf', 'gone.pdf'])
      expect(cellTexts(root, 'new-name')).toEqual(['b.pdf', 'd.pdf', 'gone.pdf'])
    })

    it('marks a problem with a named glyph, and says a ready row is ready to screen readers only', async () => {
      ipc.previewMultiRename.mockResolvedValue({ ok: true, value: MIXED })
      const root = await mountSheet()
      const status = [...root.querySelectorAll('[role="cell"][data-column="status"]')]
      expect(status[0].querySelector('[role="img"]')).toBeNull()
      expect(status[0].querySelector('.sr-only')?.textContent).toBe('Ready to rename')
      expect(status[1].querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('Name is taken')
      expect(status[2].querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('No longer in this folder')
    })

    it('“Problems only” lists the problem rows alone, paged from the backend', async () => {
      ipc.previewMultiRename.mockResolvedValue({ ok: true, value: MIXED })
      ipc.getMultiRenamePreviewRows.mockResolvedValue({ ok: true, value: MIXED.rows.slice(1) })
      const root = await mountSheet()
      const toggle = [...root.querySelectorAll<HTMLElement>('label, [role="checkbox"]')].find((el) =>
        el.textContent.includes('Problems only'),
      )
      if (!toggle) throw new Error('no Problems only toggle')
      toggle.click()
      await settle()
      expect(ipc.getMultiRenamePreviewRows).toHaveBeenLastCalledWith('S', 3, 0, 2, 'problems')
      expect(cellTexts(root, 'old-name')).toEqual(['c.pdf', 'gone.pdf'])
    })
  })

  it('inserts a placeholder into the name mask from its button', async () => {
    const root = await mountSheet()
    const counter = [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === '[C]')
    expect(counter).toBeTruthy()
    counter?.click()
    // The preview reruns after its debounce (`PREVIEW_DELAY_MS`).
    await vi.waitFor(() => {
      const lastSpec = ipc.previewMultiRename.mock.calls.at(-1)?.[1] as { nameMask: string } | undefined
      expect(lastSpec?.nameMask).toContain('[C]')
    })
  })

  describe('presets', () => {
    // `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and happy-dom reports a Linux UA.
    const macNavigator = Object.defineProperty(Object.create(navigator) as Navigator, 'userAgent', {
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)',
    })
    const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
    beforeEach(() => {
      navigatorSpy.mockReturnValue(macNavigator)
      ipc.getMultiRenamePresets.mockResolvedValue([{ id: 'p1', name: 'Mine', spec: { ...SPEC, nameMask: 'IMG_[C]' } }])
    })
    afterEach(() => navigatorSpy.mockReset())

    const SPEC = {
      nameMask: '[N]',
      extensionMask: '[E]',
      search: '',
      replace: '',
      caseSensitive: false,
      firstOnly: false,
      includeExtension: false,
      regex: false,
      substitute: false,
      case: 'unchanged',
      removeDiacritics: false,
    }

    it('F2 in a field opens the Presets menu, and 1 loads the first saved preset without renaming', async () => {
      const root = await mountSheet()
      expect(presetsButton(root).textContent).not.toContain('Mine')
      key(inputs(root)[0], 'F2', { code: 'F2' })
      await settle()
      expect(document.querySelector('[data-menu]')).toBeTruthy()

      key(document.activeElement ?? document.body, '1', { code: 'Digit1' })
      await settle()
      expect(document.querySelector('[data-menu]')).toBeNull()
      expect(inputs(root)[0].value).toBe('IMG_[C]')
      expect(presetsButton(root).textContent).toContain('Mine')
      expect(presetsButton(root).textContent).not.toContain('(edited)')
      expect(ipc.applyMultiRename).not.toHaveBeenCalled()

      const mask = inputs(root)[0]
      mask.value = 'IMG_[C]_x'
      mask.dispatchEvent(new Event('input', { bubbles: true }))
      await settle()
      expect(presetsButton(root).textContent).toContain('Mine (edited)')
    })

    it('File > Rename’s F2 accelerator opens the Presets menu, and the keydown echo of that F2 leaves it open', async () => {
      const root = await mountSheet()
      expect(runMenuClaim('file.rename')).toBe(true)
      await settle()
      expect(document.querySelector('[data-menu]')).toBeTruthy()

      // The same keypress reaching the webview too: one F2, so the menu stays open.
      key(document.activeElement ?? root, 'F2', { code: 'F2' })
      await settle()
      expect(document.querySelector('[data-menu]')).toBeTruthy()
    })

    it('⌘S asks for a name, asks before replacing a taken one, and Enter never starts a rename', async () => {
      const root = await mountSheet()
      key(inputs(root)[0], 's', { code: 'KeyS', metaKey: true })
      await settle()
      const name = document.querySelector<HTMLInputElement>('.ui-popover input')
      if (!name) throw new Error('no name popover')
      name.value = 'mine'
      name.dispatchEvent(new Event('input', { bubbles: true }))
      await settle()

      key(name, 'Enter')
      await settle()
      expect(ipc.saveMultiRenamePreset).not.toHaveBeenCalled()
      expect(document.querySelector('.ui-popover [role="alert"]')?.textContent).toContain('Mine')

      key(name, 'Enter')
      await settle()
      expect(ipc.saveMultiRenamePreset).toHaveBeenCalledWith(expect.objectContaining({ id: 'p1', name: 'mine' }))
      expect(document.querySelector('.ui-popover')).toBeNull()
      expect(ipc.applyMultiRename).not.toHaveBeenCalled()
    })

    it('Escape closes the name popover and leaves the sheet open', async () => {
      const onClose = vi.fn()
      const root = await mountSheet(vi.fn(), onClose)
      key(inputs(root)[0], 's', { code: 'KeyS', metaKey: true })
      await settle()
      const name = document.querySelector<HTMLInputElement>('.ui-popover input')
      if (!name) throw new Error('no name popover')
      key(name, 'Escape')
      await settle()
      expect(document.querySelector('.ui-popover')).toBeNull()
      expect(onClose).not.toHaveBeenCalled()
    })

    it("a saved preset's submenu renames it, prefilled with its name", async () => {
      const root = await mountSheet()
      presetsButton(root).click()
      await settle()
      key(document.activeElement ?? document.body, 'ArrowRight')
      await settle()
      key(document.activeElement ?? document.body, 'Enter')
      await settle()
      const name = document.querySelector<HTMLInputElement>('.ui-popover input')
      expect(name?.value).toBe('Mine')
      if (!name) return
      name.value = 'Ours'
      name.dispatchEvent(new Event('input', { bubbles: true }))
      key(name, 'Enter')
      await settle()
      expect(ipc.renameMultiRenamePreset).toHaveBeenCalledWith('p1', 'Ours')
    })
  })
})
