/**
 * Behaviour tests for the onboarding wizard's escape hatch.
 *
 * The safety property: someone whose Mac put Cmdr into a language they can't read
 * has to be able to get out from the first screen they ever see. So these cover
 *
 * - the way out is legible: the `English` row reads "English" while the app speaks
 *   another language, and every other row carries its own endonym;
 * - picking writes `appearance.language` through `setSetting`, the same wiring the
 *   Settings picker uses (so `settings-applier.ts` live-applies it, no restart);
 * - an explicit pick retires `'system'` for good: the tag itself is written, never a
 *   resolved tag, and a later OS language change doesn't undo it (`$lib/intl/DETAILS.md`
 *   § What `'system'` resolves to);
 * - the `'system'` row names what it currently resolves to.
 *
 * Axe coverage lives in `OnboardingLanguagePicker.a11y.test.ts`; the picker's place
 * in the wizard frame is covered by `OnboardingWizard.a11y.test.ts`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import OnboardingLanguagePicker from './OnboardingLanguagePicker.svelte'
import { _setSystemLocalesForTests } from '$lib/intl/os-locales'
import { _setLocaleForTests } from '$lib/intl/locale'

// In-memory settings store, the house pattern for a component that writes a setting
// (see `StepOptional.test.ts`). Keeps the real registry, so the options under test are
// the ones `languageOptions()` really builds.
const settingsMap: Record<string, unknown> = { 'appearance.language': 'system' }
const setSetting = vi.fn((id: string, value: unknown) => {
  settingsMap[id] = value
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => {
      setSetting(id, value)
    },
    onSpecificSettingChange: () => () => {},
  }
})

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

function mountPicker(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = { target, instance: mount(OnboardingLanguagePicker, { target, props: {} }) }
  return target
}

/** Every option row, as `[value, label]`. The menu portals to `document.body`. */
function rows(): [string, string][] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]')).map((el) => [
    el.getAttribute('data-value') ?? '',
    el.textContent.trim(),
  ])
}

function labelFor(value: string): string | undefined {
  return rows().find(([v]) => v === value)?.[1]
}

async function pick(target: HTMLElement, value: string): Promise<void> {
  target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
  await tick()
  const item = Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]')).find(
    (el) => el.getAttribute('data-value') === value,
  )
  item?.click()
  await tick()
}

beforeEach(() => {
  settingsMap['appearance.language'] = 'system'
  setSetting.mockClear()
  _setSystemLocalesForTests({ ui: null, format: null })
})

afterEach(() => {
  if (mounted) {
    void unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
  document.body.innerHTML = ''
  _setSystemLocalesForTests({ ui: null, format: null })
  _setLocaleForTests(null)
})

describe('OnboardingLanguagePicker', () => {
  it('labels every language in its own words, so the list needs no reading of the current one', async () => {
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()

    expect(labelFor('en')).toBe('English')
    expect(labelFor('hu')).toBe('Magyar')
    expect(labelFor('de')).toBe('Deutsch')
    // Both Chinese rows name their script, because the other one ships too: a
    // bare 中文 left a Traditional reader unable to tell them apart.
    expect(labelFor('zh')).toBe('简体中文')
    expect(labelFor('zh-Hant')).toBe('繁體中文')
  })

  it('keeps the English row reading "English" while the app speaks another language', async () => {
    // The whole point of the escape hatch: a user who can't read Hungarian still
    // recognizes their way out.
    _setLocaleForTests('hu')
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()

    expect(labelFor('en')).toBe('English')
  })

  it('names what "System default" currently resolves to', async () => {
    _setSystemLocalesForTests({ ui: 'sv', format: 'sv-SE' })
    const target = mountPicker()
    await tick()
    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()

    expect(labelFor('system')).toBe('System default (Svenska)')
  })

  it('writes the pick through `setSetting`, the wiring the Settings picker uses', async () => {
    const target = mountPicker()
    await tick()

    await pick(target, 'hu')

    expect(setSetting).toHaveBeenCalledWith('appearance.language', 'hu')
    expect(settingsMap['appearance.language']).toBe('hu')
  })

  it('retires `system` for good: the tag is written, never a resolved tag, and the OS stops deciding', async () => {
    _setSystemLocalesForTests({ ui: 'sv', format: 'sv-SE' })
    const target = mountPicker()
    await tick()

    await pick(target, 'hu')

    // The setting holds the user's own choice, and no write along the way smuggled in
    // the tag the OS resolved to (`'sv'`), which is what decision 5 forbids: that would
    // freeze the user out of ever following the OS again. Re-writing the `'system'`
    // sentinel is fine and does happen: `SettingSelect` applies the highlighted row as
    // a live preview, and opening the menu highlights the current value first.
    expect(settingsMap['appearance.language']).toBe('hu')
    for (const [, value] of setSetting.mock.calls) {
      expect(value === 'system' || value === 'hu').toBe(true)
    }

    // The user later moves their Mac to German. The explicit pick stands.
    _setSystemLocalesForTests({ ui: 'de', format: 'de-DE' })
    await tick()
    expect(settingsMap['appearance.language']).toBe('hu')
  })

  it("carries the language picker's accessible name from the setting registry", async () => {
    const target = mountPicker()
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('Language')
  })
})
