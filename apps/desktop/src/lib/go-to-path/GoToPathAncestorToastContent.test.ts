import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick, flushSync } from 'svelte'

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

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import GoToPathAncestorToastContent from './GoToPathAncestorToastContent.svelte'
import { getEffectiveShortcuts, removeShortcut, resetShortcut, setShortcut } from '#lib/shortcuts/shortcuts-store.js'

function setup(props: { requested: string; landed: string }) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(GoToPathAncestorToastContent, { target, props })
  return {
    target,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('GoToPathAncestorToastContent', () => {
  afterEach(() => {
    resetShortcut('nav.back')
  })

  it('renders the requested and landed paths', async () => {
    const { target, cleanup } = setup({ requested: '/tmp/nope/a.txt', landed: '/tmp' })
    await tick()
    const text = target.textContent
    expect(text).toContain('/tmp/nope/a.txt')
    expect(text).toContain('/tmp')
    cleanup()
  })

  it('shows the live nav.back binding, and follows a rebind', async () => {
    setShortcut('nav.back', 0, '⌘B')
    const { target, cleanup } = setup({ requested: '/x/y', landed: '/' })
    await tick()
    expect(target.querySelector('kbd')?.textContent).toBe('⌘B')
    setShortcut('nav.back', 0, '⌘K')
    flushSync()
    expect(target.querySelector('kbd')?.textContent).toBe('⌘K')
    cleanup()
  })

  it('omits the hint line when nav.back is unbound', async () => {
    while (getEffectiveShortcuts('nav.back').length > 0) removeShortcut('nav.back', 0)
    const { target, cleanup } = setup({ requested: '/x/y', landed: '/' })
    await tick()
    expect(target.querySelector('kbd')).toBeNull()
    expect(target.querySelector('.hint')).toBeNull()
    cleanup()
  })
})
