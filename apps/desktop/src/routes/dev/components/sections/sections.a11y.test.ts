/**
 * Tier 3 a11y tests for the Debug > Components catalog sections that have one.
 *
 * One file per section would cost about three times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its section's own doc comment and assertion.
 *
 * No stub here disagrees between blocks; each has a single consumer, and the two
 * that have a real module to fall back on spread it first.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getFileSizeFormat: () => 'binary',
  formattedDate: (t: number | null | undefined) =>
    t
      ? {
          text: '2026-05-28 10:30',
          segments: [
            { text: '2026', ageClass: 'age-fresh' as const },
            { text: '-05-28 ', ageClass: null },
            { text: '10:30', ageClass: null },
          ],
        }
      : { text: '', segments: [] },
}))

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

vi.mock('$lib/ipc/bindings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import DateLabelSection from './DateLabelSection.svelte'
import ShortcutChipSection from './ShortcutChip.svelte'
import ToggleGroupSection from './ToggleGroupSection.svelte'

// The sections share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y test for the DateLabel catalog section. Stubs the reactive-settings
 * `formattedDate` helper the same way the `DateLabel` block of
 * `$lib/ui/display.a11y.test.ts` does so jsdom can render without a live settings
 * store. Catches regressions in the section layout (caption ↔ value pairing) and
 * the underlying DateLabel markup.
 */
describe('DateLabelSection a11y', () => {
  it('renders without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DateLabelSection, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y test for the ShortcutChip catalog section. Mocks the store + bindings so
 * jsdom can render without a live shortcuts store. Catches regressions in the section
 * layout (label ↔ chip pairing) and the underlying ShortcutChip markup.
 */
describe('ShortcutChipSection a11y', () => {
  it('renders without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ShortcutChipSection, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y test for the ToggleGroup catalog section. Mirrors the convention
 * used for `lib/settings/sections/*.a11y.test.ts`: mount the section in jsdom,
 * tick once, and let axe-core audit the rendered tree. Catches regressions in
 * the example configurations (badge/hint/tooltip wiring) without needing the
 * full app.
 */
describe('ToggleGroupSection a11y', () => {
  it('renders without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToggleGroupSection, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
