/**
 * Base-locale (en) parity net for the settings i18n migration.
 *
 * The settings labels, descriptions, enum-option labels, and the
 * `{var}`-interpolated control aria-labels moved from hardcoded English into the
 * `settings.*` catalog (resolved through `t()`). This is a behavior-preserving
 * MOVE: every rendered en string must be byte-identical to the pre-migration
 * copy. These goldens are the literals that lived in `settings-registry.ts` and
 * the section components before the move; if a future copy edit is intended, it
 * lands in the catalog AND here together, never silently.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString } from '$lib/intl/messages.svelte'
import { getSettingDefinition } from './settings-registry'
import type { EnumOption } from './types'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** The option whose value matches, for asserting a moved enum-option label. */
function optionLabel(id: string, value: string | number): string {
  const def = getSettingDefinition(id)
  const opt = def?.constraints?.options?.find((o: EnumOption) => o.value === value)
  if (!opt) throw new Error(`No option ${String(value)} on ${id}`)
  return opt.label
}

describe('settings registry label/description parity (en)', () => {
  it('resolves a representative boolean setting label + description', () => {
    const def = getSettingDefinition('listing.stripedRows')
    expect(def?.label).toBe('Striped rows')
    expect(def?.description).toBe(
      'Alternate row shading for easier line tracking. Applies to both Full and Brief view modes.',
    )
  })

  it('resolves an enum setting with a multi-sentence description', () => {
    const def = getSettingDefinition('listing.sizeUnit')
    expect(def?.label).toBe('Size unit')
    expect(def?.description).toBe(
      'Dynamic picks the friendliest unit per file (1.02 MB). Fixed units make sizes apples-to-apples across the list. Bytes shows the exact count for precise comparison.',
    )
  })

  it('preserves apostrophes in copy (the ICU hazard)', () => {
    const def = getSettingDefinition('appearance.useAppIconsForDocuments')
    expect(def?.description).toBe(
      "Show the app's icon for documents instead of generic file type icons. More colorful but slightly slower.",
    )
  })

  it('preserves the MTP description with quotes and apostrophes', () => {
    const def = getSettingDefinition('fileOperations.mtpEnabled')
    expect(def?.description).toBe(
      'Detect and connect to Android and other devices over a USB cable for file browsing and transfers. To use this feature on an Android phone, you\'ll want to use a USB cable, then on your phone, go to something like Settings > USB Preferences, and set the connection to "File transfer", "Android Auto", or similar. (Varies by device.)',
    )
  })

  it('resolves an empty description to an empty string', () => {
    const def = getSettingDefinition('listing.briefColumnWidthMaxPx')
    expect(def?.description).toBe('')
  })

  it('resolves whatsNew label with a curly apostrophe (not an ICU apostrophe)', () => {
    const def = getSettingDefinition('whatsNew.showOnUpdate')
    expect(def?.label).toBe('Show what’s new after updates')
  })
})

describe('settings enum-option label parity (en)', () => {
  it('resolves theme-mode option labels (with emoji)', () => {
    expect(optionLabel('theme.mode', 'light')).toBe('☀️ Light')
    expect(optionLabel('theme.mode', 'dark')).toBe('🌙 Dark')
    expect(optionLabel('theme.mode', 'system')).toBe('💻 System')
  })

  it('resolves option labels with embedded descriptions', () => {
    const def = getSettingDefinition('appearance.dateTimeFormat')
    const iso = def?.constraints?.options?.find((o) => o.value === 'iso')
    expect(iso?.label).toBe('ISO 8601')
    expect(iso?.description).toBe('e.g., 2025-01-25 14:30')
  })

  it('resolves the file-size-format option with a parenthetical', () => {
    expect(optionLabel('appearance.fileSizeFormat', 'binary')).toBe('Binary (KiB, MiB, GiB)')
  })
})

describe('settings catalog string parity (en)', () => {
  it('resolves section titles', () => {
    expect(tString('settings.section.colorsAndFormats')).toBe('Colors and formats')
    expect(tString('settings.section.advanced')).toBe('Advanced')
    expect(tString('settings.section.smbNetworkShares')).toBe('SMB/Network shares')
  })

  it('resolves the interpolated control aria-labels', () => {
    expect(t('settings.control.decrease', { label: 'Warning threshold' })).toBe('Decrease Warning threshold')
    expect(t('settings.control.increase', { label: 'Warning threshold' })).toBe('Increase Warning threshold')
    expect(t('settings.appearance.tintSwatchAria', { label: 'Tint local-volume panes' })).toBe(
      'Choose a tint color for Tint local-volume panes',
    )
  })

  it('resolves the reset-to-default microcopy', () => {
    expect(tString('settings.control.resetToDefault')).toBe('Reset to default')
  })
})
