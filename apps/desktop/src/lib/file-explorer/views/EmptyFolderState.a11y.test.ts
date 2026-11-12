/**
 * Tier-3 a11y audit of the file lists' empty state on its own: plain text paragraphs,
 * in both shapes ("Empty folder", and the hidden-items hint with its shortcut). Where it
 * sits relative to the listbox is each list's own audit (`BriefList.a11y.test.ts`,
 * `full-list.a11y.test.ts`).
 */
import { describe, it, vi, afterEach } from 'vitest'
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

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import EmptyFolderState from './EmptyFolderState.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

describe('EmptyFolderState a11y', () => {
  const cleanups: (() => void)[] = []

  afterEach(() => {
    for (const c of cleanups) c()
    cleanups.length = 0
  })

  function render(hiddenCount: number): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(EmptyFolderState, { target, props: { hiddenCount } })
    flushSync()
    cleanups.push(() => {
      void unmount(component)
      target.remove()
    })
    return target
  }

  it('a truly empty folder has no a11y violations', async () => {
    await expectNoA11yViolations(render(0))
  })

  it('the hidden-items hint has no a11y violations', async () => {
    await expectNoA11yViolations(render(3))
  })
})
