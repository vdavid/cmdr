import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'

const {
  getRecentPathsListMock,
  loadRecentPathsMock,
  removeRecentPathMock,
  readClipboardTextMock,
  resolveGoToPathMock,
} = vi.hoisted(() => ({
  getRecentPathsListMock: vi.fn<() => { id: string; path: string; timestamp: number }[]>(() => []),
  loadRecentPathsMock: vi.fn(() => Promise.resolve()),
  removeRecentPathMock: vi.fn(() => Promise.resolve()),
  readClipboardTextMock: vi.fn<() => Promise<string | null>>(() => Promise.resolve(null)),
  resolveGoToPathMock: vi.fn<() => Promise<unknown>>(() =>
    Promise.resolve({ status: 'ok', data: { kind: 'invalid', reason: 'empty' } }),
  ),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  readClipboardText: readClipboardTextMock,
  // `./go-to-path` now pulls in the explorer store (focused-pane reads), whose
  // default tab manager reads this constant.
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { resolveGoToPath: resolveGoToPathMock },
}))

vi.mock('./recent-paths-state.svelte', () => ({
  getRecentPathsList: getRecentPathsListMock,
  loadRecentPaths: loadRecentPathsMock,
  removeRecentPath: removeRecentPathMock,
}))

import GoToPathDialog from './GoToPathDialog.svelte'
import type { GoToPathResolution } from '$lib/ipc/bindings'

type DialogProps = ComponentProps<typeof GoToPathDialog>
type GoFn = (input: string) => Promise<GoToPathResolution | undefined>

/** Drain the microtask + Svelte tick queue enough for the onMount async chain. */
async function flush(times = 8): Promise<void> {
  for (let i = 0; i < times; i++) {
    await Promise.resolve()
    await tick()
  }
}

/** A typed `onGo` mock that always resolves to `resolution`. */
function goMock(resolution: GoToPathResolution): ReturnType<typeof vi.fn<GoFn>> {
  return vi.fn<GoFn>(() => Promise.resolve(resolution))
}

function setup(overrides: { baseDir?: string; onGo?: ReturnType<typeof vi.fn<GoFn>>; onCancel?: () => void } = {}) {
  const onGo = overrides.onGo ?? goMock({ kind: 'directory', path: '/x' })
  const onCancel = overrides.onCancel ?? vi.fn()
  const target = document.createElement('div')
  document.body.appendChild(target)
  const props: DialogProps = { baseDir: overrides.baseDir ?? '/home/me', onGo, onCancel }
  mount(GoToPathDialog, { target, props })
  return {
    target,
    onGo,
    onCancel,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('GoToPathDialog', () => {
  beforeEach(() => {
    getRecentPathsListMock.mockReset().mockReturnValue([])
    loadRecentPathsMock.mockReset().mockResolvedValue(undefined)
    removeRecentPathMock.mockReset().mockResolvedValue(undefined)
    readClipboardTextMock.mockReset().mockResolvedValue(null)
    resolveGoToPathMock.mockReset().mockResolvedValue({ status: 'ok', data: { kind: 'invalid', reason: 'empty' } })
  })

  it('disables "Go to path" when the box is empty, enables it once typed', async () => {
    const { target, cleanup } = setup()
    await tick()
    const goButton = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Go to path')
    expect(goButton?.disabled).toBe(true)

    const input = target.querySelector('input') as HTMLInputElement
    input.value = '/tmp'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(goButton?.disabled).toBe(false)
    cleanup()
  })

  it('renders up to the recents with digit chips (1..9, 0)', async () => {
    getRecentPathsListMock.mockReturnValue(
      Array.from({ length: 11 }, (_, i) => ({ id: `id${String(i)}`, path: `/p${String(i)}`, timestamp: i })),
    )
    const { target, cleanup } = setup()
    await tick()
    const rows = target.querySelectorAll('.recent-row')
    expect(rows).toHaveLength(10) // cap 10
    const chips = [...target.querySelectorAll('.digit-chip')].map((c) => c.textContent)
    expect(chips).toEqual(['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'])
    cleanup()
  })

  it('clicking a recent row jumps and closes the dialog', async () => {
    getRecentPathsListMock.mockReturnValue([{ id: 'a', path: '/recent/one', timestamp: 1 }])
    const onGo = goMock({ kind: 'directory', path: '/recent/one' })
    const { target, onCancel, cleanup } = setup({ onGo })
    await tick()
    const row = target.querySelector('.recent-main') as HTMLButtonElement
    row.click()
    await tick()
    await tick()
    expect(onGo).toHaveBeenCalledWith('/recent/one')
    expect(onCancel).toHaveBeenCalled()
    cleanup()
  })

  it('pressing a digit while the box is empty jumps to that recent', async () => {
    getRecentPathsListMock.mockReturnValue([
      { id: 'a', path: '/first', timestamp: 1 },
      { id: 'b', path: '/second', timestamp: 2 },
    ])
    const onGo = goMock({ kind: 'directory', path: '/second' })
    const { target, cleanup } = setup({ onGo })
    await tick()
    const input = target.querySelector('input') as HTMLInputElement
    input.dispatchEvent(new KeyboardEvent('keydown', { key: '2', bubbles: true }))
    await tick()
    await tick()
    expect(onGo).toHaveBeenCalledWith('/second')
    cleanup()
  })

  it('a digit with text in the box is ordinary input (no jump)', async () => {
    getRecentPathsListMock.mockReturnValue([{ id: 'a', path: '/first', timestamp: 1 }])
    const onGo = goMock({ kind: 'directory', path: '/x' })
    const { target, cleanup } = setup({ onGo })
    await tick()
    const input = target.querySelector('input') as HTMLInputElement
    input.value = '/tmp'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: '1', bubbles: true }))
    await tick()
    expect(onGo).not.toHaveBeenCalled()
    cleanup()
  })

  it('the remove button removes the entry without jumping', async () => {
    getRecentPathsListMock.mockReturnValue([{ id: 'a', path: '/first', timestamp: 1 }])
    const onGo = goMock({ kind: 'directory', path: '/x' })
    const { target, cleanup } = setup({ onGo })
    await tick()
    const removeButton = target.querySelector('.remove-button') as HTMLButtonElement
    removeButton.click()
    await tick()
    expect(removeRecentPathMock).toHaveBeenCalledWith('a')
    expect(onGo).not.toHaveBeenCalled()
    cleanup()
  })

  it('the remove button is keyboard-reachable and operable', async () => {
    getRecentPathsListMock.mockReturnValue([{ id: 'a', path: '/first', timestamp: 1 }])
    const onGo = goMock({ kind: 'directory', path: '/x' })
    const { target, cleanup } = setup({ onGo })
    await tick()
    const removeButton = target.querySelector('.remove-button') as HTMLButtonElement
    // A real `<button>` in the natural tab order: no negative tabindex removing
    // it. Keyboard-only users can focus and operate it (Enter/Space dispatch a
    // native click on a button), so it must not be excluded from tabbing.
    expect(removeButton.getAttribute('tabindex')).toBeNull()
    expect(removeButton.getAttribute('aria-label')).toBe('Remove from list')
    // Enter/Space on a focused native button fire a `click`; simulate that path.
    removeButton.focus()
    expect(target.contains(document.activeElement)).toBe(true)
    removeButton.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    expect(removeRecentPathMock).toHaveBeenCalledWith('a')
    expect(onGo).not.toHaveBeenCalled()
    cleanup()
  })

  it('Enter confirms: jumps and closes on a directory outcome', async () => {
    const onGo = goMock({ kind: 'directory', path: '/typed' })
    const { target, onCancel, cleanup } = setup({ onGo })
    await tick()
    const input = target.querySelector('input') as HTMLInputElement
    input.value = '/typed'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()
    await tick()
    expect(onGo).toHaveBeenCalledWith('/typed')
    expect(onCancel).toHaveBeenCalled()
    cleanup()
  })

  it('Enter on an invalid outcome keeps the dialog open', async () => {
    const onGo = goMock({ kind: 'invalid', reason: 'empty' })
    const { target, onCancel, cleanup } = setup({ onGo })
    await tick()
    const input = target.querySelector('input') as HTMLInputElement
    input.value = '???'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()
    await tick()
    expect(onGo).toHaveBeenCalled()
    expect(onCancel).not.toHaveBeenCalled()
    cleanup()
  })

  it('prefills the box from a clipboard path that resolves to a real target', async () => {
    readClipboardTextMock.mockResolvedValue('/Users/me/Documents')
    resolveGoToPathMock.mockResolvedValue({ status: 'ok', data: { kind: 'directory', path: '/Users/me/Documents' } })
    const { target, cleanup } = setup()
    await flush()
    const input = target.querySelector('input') as HTMLInputElement
    expect(input.value).toBe('/Users/me/Documents')
    cleanup()
  })

  it('does not prefill when the clipboard path does not exist', async () => {
    readClipboardTextMock.mockResolvedValue('/nope/missing')
    resolveGoToPathMock.mockResolvedValue({
      status: 'ok',
      data: { kind: 'nearestAncestor', requested: '/nope/missing', ancestorDir: '/' },
    })
    const { target, cleanup } = setup()
    await flush()
    const input = target.querySelector('input') as HTMLInputElement
    expect(input.value).toBe('')
    cleanup()
  })
})
