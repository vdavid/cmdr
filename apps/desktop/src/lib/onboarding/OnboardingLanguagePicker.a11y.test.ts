/**
 * Tier 3 axe a11y tests for `OnboardingLanguagePicker.svelte`.
 *
 * Three states: closed on the `'system'` default, closed on an explicit pick, and open
 * with the menu rendered. The glyph is decorative (`aria-hidden`), so the trigger's
 * accessible name is the setting's own label; the open menu is Ark's listbox. Axe runs
 * in jsdom, so no contrast checks here (tier-1 scripts cover those).
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync } from 'svelte'
import OnboardingLanguagePicker from './OnboardingLanguagePicker.svelte'
import { _setSystemLocalesForTests } from '$lib/intl/os-locales'
import { expectNoA11yViolations } from '$lib/test-a11y'

const settingsMap: Record<string, unknown> = { 'appearance.language': 'system' }

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => {
      settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

function mountPicker(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  // Portal the menu into `target` so axe sees the whole control in one tree, the way
  // the wizard portals it into its own overlay.
  mounted = { target, instance: mount(OnboardingLanguagePicker, { target, props: { portalContainer: target } }) }
  return target
}

beforeEach(() => {
  settingsMap['appearance.language'] = 'system'
  _setSystemLocalesForTests({ ui: null, format: null })
})

afterEach(async () => {
  if (mounted) {
    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
  document.body.innerHTML = ''
  _setSystemLocalesForTests({ ui: null, format: null })
})

describe('OnboardingLanguagePicker a11y', () => {
  it('closed, on the System default, has no a11y violations', async () => {
    const target = mountPicker()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('closed, on an explicit pick, has no a11y violations', async () => {
    settingsMap['appearance.language'] = 'hu'
    const target = mountPicker()
    await settle()
    await expectNoA11yViolations(target)
  })

  it('open, with the language menu rendered, has no a11y violations', async () => {
    const target = mountPicker()
    await settle()
    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await settle()
    await expectNoA11yViolations(target)
  })
})
