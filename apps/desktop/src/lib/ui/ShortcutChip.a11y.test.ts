/**
 * Tier 3 a11y test for ShortcutChip. Covers both rendered shapes: the non-clickable
 * <kbd> and the clickable <button> (which must carry an accessible name). Mocks the
 * store + bindings the same way the behavior test does so jsdom can render without a
 * live shortcuts store.
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

import ShortcutChip from './ShortcutChip.svelte'

async function mountChip(props: Record<string, unknown>): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ShortcutChip, { target, props })
  await tick()
  return target
}

describe('ShortcutChip a11y', () => {
  it('literal (non-clickable) chip has no a11y violations', async () => {
    const target = await mountChip({ key: '⏎' })
    await expectNoA11yViolations(target)
  })

  it('clickable commandId chip has no a11y violations', async () => {
    const target = await mountChip({ commandId: 'downloads.goToLatest' })
    await expectNoA11yViolations(target)
  })

  it('non-clickable commandId chip has no a11y violations', async () => {
    const target = await mountChip({ commandId: 'downloads.goToLatest', clickable: false })
    await expectNoA11yViolations(target)
  })
})
