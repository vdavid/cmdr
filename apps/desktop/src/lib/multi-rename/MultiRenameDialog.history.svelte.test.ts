/**
 * The text fields' history (⌥⇧↓), the sheet side: the key in a field opens that field's
 * earlier values newest first, a pick fills the field, a field with no history says so, plain
 * ↓ stays the mask's own, and a field with history shows a quiet hint on hover or focus.
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

/** ⌥⇧↓ as macOS sends it. */
function pressHistory(input: HTMLElement): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key: 'ArrowDown',
    code: 'ArrowDown',
    altKey: true,
    shiftKey: true,
    bubbles: true,
    cancelable: true,
  })
  input.dispatchEvent(event)
  return event
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

  it('⌥⇧↓ in a field lists that field’s values, newest first, and a pick fills the field', async () => {
    const root = await mountSheet()
    const search = fields(root)[2]
    const event = pressHistory(search)
    await settle()
    expect(event.defaultPrevented).toBe(true)
    expect(menuRows()).toEqual(['IMG', 'DSC'])

    document.querySelector<HTMLElement>('[data-menu] [role="menuitem"]')?.click()
    await new Promise((resolve) => setTimeout(resolve, 150))
    await settle()
    expect(document.querySelector('[data-menu]')).toBeNull()
    expect(fields(root)[2].value).toBe('IMG')
    expect(ipc.previewMultiRename).toHaveBeenLastCalledWith('S', expect.objectContaining({ search: 'IMG' }))
  })

  it('a field with no history yet says so', async () => {
    const root = await mountSheet()
    pressHistory(fields(root)[3])
    await settle()
    expect(menuRows()).toEqual(['Nothing here yet. A value is kept when a rename runs.'])
  })

  it('plain ↓ in the name mask stays the mask’s own', async () => {
    const root = await mountSheet()
    fields(root)[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', code: 'ArrowDown', bubbles: true }))
    await settle()
    expect(document.querySelector('[data-menu]')).toBeNull()
  })

  it('shows a quiet hint only in a field with history, and a click on it opens the list', async () => {
    const root = await mountSheet()
    const hints = [...root.querySelectorAll<HTMLButtonElement>('.history-hint')]
    expect(hints).toHaveLength(2)
    expect(hints[0].closest('.field')?.contains(fields(root)[0])).toBe(true)
    expect(hints[1].closest('.field')?.contains(fields(root)[2])).toBe(true)
    expect(hints[0].tabIndex).toBe(-1)

    hints[1].click()
    await settle()
    expect(menuRows()).toEqual(['IMG', 'DSC'])
  })
})
