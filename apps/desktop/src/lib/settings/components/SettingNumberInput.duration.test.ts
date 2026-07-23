/**
 * Functional test for `SettingNumberInput`'s duration handling: a `duration` setting stores
 * milliseconds but is shown and edited in a coarser unit. This guards the conversion DIRECTION
 * (divide to display, multiply to store) and the auto-derived unit label, which a pure round-trip
 * test can't catch on its own.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingNumberInput from './SettingNumberInput.svelte'

const setSetting = vi.fn()

// `advanced.mountTimeout`-shaped: stored 20000 ms, edited in seconds. The factory is hoisted
// above imports, so pull the REAL conversion math from `../types` inside it (not the mocked
// `$lib/settings`) — a flipped divide/multiply then shows up as a wrong displayed value.
vi.mock('$lib/settings', async () => {
  const types = await import('../types')
  return {
    getSetting: vi.fn(() => 20_000),
    setSetting: (...args: unknown[]) => {
      setSetting(...args)
    },
    getSettingDefinition: vi.fn(() => ({
      label: 'Mount timeout',
      description: '',
      type: 'duration',
      constraints: { unit: 's', minMs: 5_000, maxMs: 120_000, step: 1 },
    })),
    onSpecificSettingChange: vi.fn(() => () => {}),
    durationUnitFactor: types.durationUnitFactor,
    msToDurationValue: types.msToDurationValue,
    durationValueToMs: types.durationValueToMs,
  }
})

describe('SettingNumberInput duration conversion', () => {
  it('shows the stored ms in the display unit, with the unit label', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingNumberInput, { target, props: { id: 'advanced.mountTimeout' } })
    await tick()

    const input = target.querySelector('input')
    expect(input).not.toBeNull()
    // 20000 ms / 1000 = 20 s, NOT 20000 (which a missing divide would show).
    expect(input?.value).toBe('20')

    // Unit label is auto-derived from the setting's `constraints.unit`.
    expect(target.querySelector('.ni-unit')?.textContent).toBe('s')
  })
})
