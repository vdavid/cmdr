/**
 * Behavior tests for `SettingColorSwatchPicker.svelte` covering the
 * trigger/popover lifecycle and the writes back to the settings store.
 *
 * The pure keyboard helper has its own tests in `swatch-keyboard.test.ts`,
 * and the a11y tier lives in `SettingColorSwatchPicker.a11y.test.ts`.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingColorSwatchPicker from './SettingColorSwatchPicker.svelte'

let currentValue: string = 'none'
const setSetting = vi.fn(async (_id: string, v: string) => {
  currentValue = v
})

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => currentValue),
  setSetting: (id: string, v: string) => setSetting(id, v),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

beforeEach(() => {
  currentValue = 'none'
  setSetting.mockClear()
  document.body.innerHTML = ''
})

function mountPicker() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SettingColorSwatchPicker, {
    target,
    props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
  })
  return target
}

describe('SettingColorSwatchPicker', () => {
  it('starts closed (no popover in the DOM)', async () => {
    const target = mountPicker()
    await tick()
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('exposes the current value in the aria-label of the trigger', async () => {
    currentValue = 'blue'
    const target = mountPicker()
    await tick()
    const trigger = target.querySelector<HTMLButtonElement>('button.trigger')
    expect(trigger?.getAttribute('aria-label')).toContain('Blue')
  })

  it('opens the popover on click and renders 13 swatches (12 + none)', async () => {
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    const dialog = target.querySelector('[role="dialog"]')
    expect(dialog).not.toBeNull()
    const swatches = target.querySelectorAll('button[role="option"]')
    expect(swatches.length).toBe(13)
  })

  it('writes the chosen color to the settings store and closes', async () => {
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    const blueSwatch = target.querySelector<HTMLButtonElement>('[aria-label="Blue"]')
    expect(blueSwatch).not.toBeNull()
    blueSwatch?.click()
    await tick()
    expect(setSetting).toHaveBeenCalledWith('appearance.tintLocal', 'blue')
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('marks the current value as aria-selected in the open popover', async () => {
    currentValue = 'cyan'
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    const selected = target.querySelectorAll('button[aria-selected="true"]')
    expect(selected.length).toBe(1)
    expect(selected[0]?.getAttribute('aria-label')).toBe('Cyan')
  })

  it('opens via Enter on the trigger and closes via Escape', async () => {
    const target = mountPicker()
    await tick()
    const trigger = target.querySelector<HTMLButtonElement>('button.trigger')
    trigger?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()
    expect(target.querySelector('[role="dialog"]')).not.toBeNull()

    const dialog = target.querySelector<HTMLDivElement>('[role="dialog"]')
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('opens via ArrowDown on the trigger', async () => {
    const target = mountPicker()
    await tick()
    const trigger = target.querySelector<HTMLButtonElement>('button.trigger')
    trigger?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    await tick()
    expect(target.querySelector('[role="dialog"]')).not.toBeNull()
  })

  it('closes when clicking outside the popover', async () => {
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    expect(target.querySelector('[role="dialog"]')).not.toBeNull()

    // Click outside (on document body, far from the picker)
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    await tick()
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('selecting "No tint" stores "none"', async () => {
    currentValue = 'red'
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    const noTint = target.querySelector<HTMLButtonElement>('[aria-label="No tint"]')
    noTint?.click()
    await tick()
    expect(setSetting).toHaveBeenCalledWith('appearance.tintLocal', 'none')
  })
})
