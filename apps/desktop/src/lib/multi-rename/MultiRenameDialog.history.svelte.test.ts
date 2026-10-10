/**
 * The text fields' history, the sheet side: plain ↓ in a field, or the chevron at its end, opens
 * that field's earlier values newest first, and a pick fills the field. In a mask, ↓ with the caret
 * in a `[C…]` token stays the counter editor's. A field with no history has a disabled chevron.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import { setLastMultiRenameRun } from './last-run.svelte'

const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  getMultiRenamePreviewRows: vi.fn(),
  renderMultiRenameExamples: vi.fn(),
  applyMultiRename: vi.fn(),
  getMultiRenamePresets: vi.fn(),
  getMultiRenameLastSettings: vi.fn(),
  saveMultiRenameLastSettings: vi.fn(),
  getMultiRenameHistory: vi.fn(),
  rollbackOperation: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  ...ipc,
}))

const PREVIEW = { previewId: 1, counts: { ready: 0, unchanged: 0, problems: 0 }, rows: [] }

const HISTORY = [
  { id: '1', field: 'search', value: 'IMG' },
  { id: '2', field: 'nameMask', value: '[N]_[C]' },
  { id: '3', field: 'search', value: 'DSC' },
]

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let mounted: ReturnType<typeof mount>[] = []

async function mountSheet(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted.push(
    mount(MultiRenameDialog, {
      target,
      props: { session: { sessionId: 'S', count: 1 }, onApplied: vi.fn(), onUndoStarted: vi.fn(), onClose: vi.fn() },
    }),
  )
  await settle()
  return target
}

/** The four text fields: name mask, extension mask, search, replace. */
function fields(root: HTMLElement): HTMLInputElement[] {
  return [...root.querySelectorAll<HTMLInputElement>('input:not([type="checkbox"])')]
}

/** Plain ↓, from `input` with the caret at `caret` (its end by default). */
function pressDown(input: HTMLInputElement, caret = input.value.length): KeyboardEvent {
  input.focus()
  input.setSelectionRange(caret, caret)
  const event = new KeyboardEvent('keydown', { key: 'ArrowDown', code: 'ArrowDown', bubbles: true, cancelable: true })
  input.dispatchEvent(event)
  return event
}

function chevron(root: HTMLElement, field: number): HTMLButtonElement {
  const button = fields(root)[field].closest('.field')?.querySelector<HTMLButtonElement>('.history-chevron')
  if (!button) throw new Error(`no chevron in field ${String(field)}`)
  return button
}

function menuRows(): string[] {
  return [...document.querySelectorAll('[data-menu] [role="menuitem"]')].map((el) => el.textContent.trim())
}

describe('MultiRenameDialog field history', () => {
  const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')

  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: PREVIEW })
    ipc.renderMultiRenameExamples.mockImplementation((examples: unknown[]) => Promise.resolve(examples.map(() => null)))
    ipc.getMultiRenamePresets.mockResolvedValue([])
    ipc.getMultiRenameLastSettings.mockResolvedValue(null)
    ipc.saveMultiRenameLastSettings.mockResolvedValue(undefined)
    ipc.getMultiRenameHistory.mockResolvedValue(HISTORY)
    setLastMultiRenameRun(null)
  })
  afterEach(async () => {
    for (const sheet of mounted) await unmount(sheet)
    mounted = []
    navigatorSpy.mockReset()
  })

  it('↓ in a field lists that field’s values, newest first, and a pick fills the field', async () => {
    const root = await mountSheet()
    const event = pressDown(fields(root)[2])
    await settle()
    expect(event.defaultPrevented).toBe(true)
    expect(menuRows()).toEqual(['IMG', 'DSC'])
    // Regression: portaled to body it sat under the modal's layer, invisible.
    expect(document.querySelector('[data-menu]')?.parentElement).not.toBe(document.body)

    document.querySelector<HTMLElement>('[data-menu] [role="menuitem"]')?.click()
    await new Promise((resolve) => setTimeout(resolve, 150))
    await settle()
    expect(document.querySelector('[data-menu]')).toBeNull()
    expect(fields(root)[2].value).toBe('IMG')
    expect(ipc.previewMultiRename).toHaveBeenLastCalledWith('S', expect.objectContaining({ search: 'IMG' }))
  })

  it('↓ in a mask opens its history with the caret away from a counter', async () => {
    const root = await mountSheet()
    const mask = fields(root)[0]
    mask.value = 'x [C] y'
    mask.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    pressDown(mask, 1)
    await settle()
    expect(menuRows()).toEqual(['[N]_[C]'])
  })

  it('↓ with the caret in a counter token opens the counter editor, not the history', async () => {
    const root = await mountSheet()
    const mask = fields(root)[0]
    mask.value = 'x [C] y'
    mask.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    pressDown(mask, 4)
    await settle()
    expect(document.querySelector('[data-menu]')).toBeNull()
    expect(document.querySelectorAll('.ui-popover input').length).toBeGreaterThan(0)
  })

  it('↓ in a field with no history is left alone', async () => {
    const root = await mountSheet()
    const event = pressDown(fields(root)[3])
    await settle()
    expect(event.defaultPrevented).toBe(false)
    expect(document.querySelector('[data-menu]')).toBeNull()
  })

  it('each field ends in a chevron that opens its list, named and stated, and disabled with no history', async () => {
    const root = await mountSheet()
    const search = chevron(root, 2)
    expect(search.getAttribute('aria-haspopup')).toBe('menu')
    expect(search.getAttribute('aria-label')).toBe('History')
    expect(search.getAttribute('aria-expanded')).toBe('false')
    expect(search.disabled).toBe(false)
    expect(chevron(root, 1).disabled).toBe(true)
    expect(chevron(root, 3).disabled).toBe(true)

    search.click()
    await settle()
    expect(menuRows()).toEqual(['IMG', 'DSC'])
    expect(search.getAttribute('aria-expanded')).toBe('true')
  })
})
