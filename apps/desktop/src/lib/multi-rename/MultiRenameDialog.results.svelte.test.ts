/**
 * Results (⌥⏎), the sheet side: it writes the shown preview to a file and opens it in the
 * text editor, coming back to the window reads the user's edits and re-previews, the line
 * under the fields says how many names came from the list, a row with a typed name carries
 * a mark, and "Use the settings again" drops them.
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
  writeMultiRenameNames: vi.fn(),
  readMultiRenameNames: vi.fn(),
  clearMultiRenameNames: vi.fn(),
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

const editor = vi.hoisted(() => ({ openFileInEditor: vi.fn() }))
vi.mock('$lib/text-editor/open-file-in-editor', () => editor)

function row(oldName: string, newName: string, edited = false) {
  return { row: 0, oldName, newName, status: { type: 'ready' }, iconId: null, isDirectory: false, edited }
}

const PREVIEW = { previewId: 4, counts: { ready: 1, unchanged: 0, problems: 0 }, rows: [row('a.txt', 'a-1.txt')] }
const EDITED = { previewId: 5, counts: { ready: 1, unchanged: 0, problems: 0 }, rows: [row('a.txt', 'ay.txt', true)] }

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

/** Past the preview's debounce, so the re-preview a read-back asks for has landed. */
async function settlePreview(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 150))
  await settle()
}

/** The mounted sheets, unmounted after each test so no window listener outlives it. */
let mounted: ReturnType<typeof mount>[] = []

async function mountSheet(onApplied = vi.fn()): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted.push(
    mount(MultiRenameDialog, {
      target,
      props: { session: { sessionId: 'S', count: 1 }, onApplied, onUndoStarted: vi.fn(), onClose: vi.fn() },
    }),
  )
  await settle()
  return target
}

function nameMask(root: HTMLElement): HTMLInputElement {
  const input = root.querySelector<HTMLInputElement>('input:not([type="checkbox"])')
  if (!input) throw new Error('no name mask')
  return input
}

/** ⌥⏎ as macOS sends it, from the name mask. */
function pressResults(root: HTMLElement): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key: 'Enter',
    code: 'Enter',
    altKey: true,
    bubbles: true,
    cancelable: true,
  })
  nameMask(root).dispatchEvent(event)
  return event
}

/** A footer button by its label, ahead of its key chip. */
function footerButton(root: Element, label: string): HTMLButtonElement | undefined {
  return [...root.querySelectorAll<HTMLButtonElement>('button.btn')].find((b) => b.textContent.trim().startsWith(label))
}

function link(root: Element, text: string): HTMLButtonElement | undefined {
  return [...root.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent.trim() === text)
}

function notice(root: HTMLElement): string {
  return root.querySelector('.results-notice')?.textContent.replace(/\s+/g, ' ').trim() ?? ''
}

describe('MultiRenameDialog Results', () => {
  // `formatKeyCombo` emits ⌥-form modifiers only when `isMacOS()` is true, and happy-dom reports a Linux UA.
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
    ipc.getMultiRenameHistory.mockResolvedValue([])
    ipc.writeMultiRenameNames.mockResolvedValue({ ok: true, value: '/tmp/cmdr-multi-rename/S.txt' })
    ipc.readMultiRenameNames.mockResolvedValue({ ok: true, value: 1 })
    ipc.clearMultiRenameNames.mockResolvedValue({ ok: true, value: null })
    editor.openFileInEditor.mockResolvedValue(true)
    setLastMultiRenameRun(null)
  })
  afterEach(async () => {
    for (const sheet of mounted) await unmount(sheet)
    mounted = []
    navigatorSpy.mockReset()
  })

  it('⌥⏎ writes the shown preview’s names and opens them in the text editor, and never renames', async () => {
    const root = await mountSheet()
    const event = pressResults(root)
    await settle()
    expect(event.defaultPrevented).toBe(true)
    expect(ipc.writeMultiRenameNames).toHaveBeenCalledWith('S', 4)
    expect(editor.openFileInEditor).toHaveBeenCalledWith('/tmp/cmdr-multi-rename/S.txt')
    expect(ipc.applyMultiRename).not.toHaveBeenCalled()
    expect(notice(root)).toContain('Edit the new names in your text editor')
  })

  it('the footer’s Results… button, its key chip on it, does the same', async () => {
    const root = await mountSheet()
    const results = footerButton(root, 'Results…')
    if (!results) throw new Error('no Results… button')
    expect(results.classList.contains('btn')).toBe(true)
    expect(results.querySelector('.shortcut-chip')?.textContent).toBe('⌥↩')
    results.click()
    await settle()
    expect(editor.openFileInEditor).toHaveBeenCalled()
  })

  it('coming back to the window reads the names back, re-previews, and marks the typed row', async () => {
    const root = await mountSheet()
    pressResults(root)
    await settle()
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: EDITED })

    window.dispatchEvent(new FocusEvent('focus'))
    await settlePreview()

    expect(ipc.readMultiRenameNames).toHaveBeenCalledWith('S')
    expect(notice(root)).toContain('1 name comes from your edited list')
    const mark = root.querySelector('[role="cell"][data-column="arrow"] .edited-mark')
    expect(mark?.textContent).toContain('You typed this name in Results')
  })

  it('focus does nothing before Results opened a file', async () => {
    await mountSheet()
    window.dispatchEvent(new FocusEvent('focus'))
    await settle()
    expect(ipc.readMultiRenameNames).not.toHaveBeenCalled()
  })

  it('“Use the settings again” drops the typed names and stops reading the file', async () => {
    const root = await mountSheet()
    pressResults(root)
    await settle()
    window.dispatchEvent(new FocusEvent('focus'))
    await settlePreview()

    link(root, 'Use the settings again')?.click()
    await settlePreview()

    expect(ipc.clearMultiRenameNames).toHaveBeenCalledWith('S')
    expect(notice(root)).toBe('')
    ipc.readMultiRenameNames.mockClear()
    window.dispatchEvent(new FocusEvent('focus'))
    await settle()
    expect(ipc.readMultiRenameNames).not.toHaveBeenCalled()
  })

  it('says so when the names list is gone', async () => {
    ipc.readMultiRenameNames.mockResolvedValue({ ok: false, error: { type: 'namesFileGone', detail: 'x' } })
    const root = await mountSheet()
    pressResults(root)
    await settle()
    window.dispatchEvent(new FocusEvent('focus'))
    await settle()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('The names list is gone')
  })

  it('words a names file it couldn’t write in the error line', async () => {
    ipc.writeMultiRenameNames.mockResolvedValue({ ok: false, error: { type: 'couldntWriteNames', detail: 'x' } })
    const root = await mountSheet()
    pressResults(root)
    await settle()
    expect(editor.openFileInEditor).not.toHaveBeenCalled()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('Couldn’t write the names list')
  })
})
