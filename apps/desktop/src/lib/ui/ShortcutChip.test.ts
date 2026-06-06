/**
 * Behavior tests for ShortcutChip.
 *
 * The two modes carry different contracts: a `key` chip is fixed typography, a
 * `commandId` chip is a live claim about the effective binding (must track rebinds and
 * vanish when unbound). The clickable variant deep-links into Settings via a lazy
 * import, mocked here.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

const openShortcutCustomization = vi.fn(() => Promise.resolve())
// The component imports this lazily via dynamic `import()` (keeps the Tauri
// window-creation surface out of the viewer bundle). Mock the same specifier.
vi.mock('$lib/settings/settings-window', () => ({
  openShortcutCustomization,
}))

import ShortcutChip from './ShortcutChip.svelte'
import { setShortcut, resetShortcut } from '$lib/shortcuts/shortcuts-store'

function setup(props: Record<string, unknown>) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(ShortcutChip, { target, props })
  flushSync()
  return { target, component }
}

describe('ShortcutChip', () => {
  const cleanups: (() => void)[] = []

  afterEach(() => {
    for (const c of cleanups) c()
    cleanups.length = 0
    resetShortcut('downloads.goToLatest')
    openShortcutCustomization.mockClear()
  })

  function setupTracked(props: Record<string, unknown>) {
    const result = setup(props)
    cleanups.push(() => {
      void unmount(result.component)
      result.target.remove()
    })
    return result
  }

  it('renders the literal key in literal mode', () => {
    const { target } = setupTracked({ key: '⏎' })
    const kbd = target.querySelector('kbd')
    expect(kbd?.textContent).toBe('⏎')
    expect(target.querySelector('button')).toBeNull()
  })

  it('renders the effective first shortcut for a bound command', () => {
    // downloads.goToLatest defaults to ⌘J (rendered as Ctrl+J off macOS, where the
    // test platform formats it). Assert the load-bearing key letter, platform-agnostic.
    const { target } = setupTracked({ commandId: 'downloads.goToLatest' })
    expect(target.querySelector('kbd')?.textContent).toContain('J')
  })

  it('renders nothing when the command has no binding', () => {
    // app.about has no registry shortcut.
    const { target } = setupTracked({ commandId: 'app.about' })
    expect(target.querySelector('kbd')).toBeNull()
    expect(target.querySelector('button')).toBeNull()
  })

  it('updates the rendered chip live when the user rebinds the command', () => {
    const { target } = setupTracked({ commandId: 'downloads.goToLatest' })
    expect(target.querySelector('kbd')?.textContent).toContain('J')

    // A custom shortcut is stored verbatim, so it renders exactly as set.
    setShortcut('downloads.goToLatest', 0, '⌘9')
    flushSync()

    expect(target.querySelector('kbd')?.textContent).toBe('⌘9')
  })

  it('is clickable by default in commandId mode and opens the customization deep link', async () => {
    const { target } = setupTracked({ commandId: 'downloads.goToLatest' })
    const button = target.querySelector('button')
    expect(button).not.toBeNull()
    expect(button?.getAttribute('aria-label')).toBe('Customize the Go to latest download shortcut')

    button?.click()
    // The dynamic import resolves on a microtask; let the event loop drain.
    await vi.waitFor(() => { expect(openShortcutCustomization).toHaveBeenCalledWith('downloads.goToLatest'); })
  })

  it('renders a non-clickable kbd when clickable is false', () => {
    const { target } = setupTracked({ commandId: 'downloads.goToLatest', clickable: false })
    expect(target.querySelector('button')).toBeNull()
    expect(target.querySelector('kbd')?.textContent).toContain('J')
  })

  it('a literal chip is never a button even with clickable set', () => {
    const { target } = setupTracked({ key: '⏎', clickable: true })
    expect(target.querySelector('button')).toBeNull()
    expect(target.querySelector('kbd')?.textContent).toBe('⏎')
  })
})
