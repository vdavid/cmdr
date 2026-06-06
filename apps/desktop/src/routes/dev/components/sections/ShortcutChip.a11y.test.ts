/**
 * Tier 3 a11y test for the ShortcutChip catalog section. Mocks the store + bindings so
 * jsdom can render without a live shortcuts store. Catches regressions in the section
 * layout (label ↔ chip pairing) and the underlying ShortcutChip markup.
 */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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

import ShortcutChipSection from './ShortcutChip.svelte'

describe('ShortcutChipSection a11y', () => {
  it('renders without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ShortcutChipSection, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
