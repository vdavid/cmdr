/**
 * Tier 3 a11y tests for the onboarding pieces that mount on their own: the step
 * shell, the language picker, and the two cloud-provider controls.
 *
 * One file per component would cost about four times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * The wizard and its steps live in `onboarding-steps.a11y.test.ts`; one merged
 * file for the whole directory would clear the 800-line `file-length` mark.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount, flushSync, createRawSnippet } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { _setSystemLocalesForTests } from '$lib/intl/os-locales'

// The two blocks that stub settings keep disjoint keys, so one map serves both;
// each resets its own in `beforeEach`. `getSetting` is per-block because the
// picker answers `undefined` for an unknown key and the setup answers `''`.
const stubs = vi.hoisted(() => ({
  settingsMap: Object.create(null) as Record<string, unknown>,
  getSetting: null as ((id: string) => unknown) | null,
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realGetSetting = actual.getSetting as (id: string) => unknown
  return {
    ...actual,
    getSetting: (id: string) => (stubs.getSetting ? stubs.getSetting(id) : realGetSetting(id)),
    setSetting: (id: string, value: unknown) => {
      stubs.settingsMap[id] = value
    },
    onSpecificSettingChange: () => () => {},
  }
})

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  checkAiConnection: vi.fn(() => Promise.resolve({ connected: false, authError: false, models: [], error: null })),
  saveAiApiKey: vi.fn(() => Promise.resolve(null)),
  getAiApiKeyStatus: vi.fn(() => Promise.resolve({ isSet: false, fingerprint: '' })),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

import CloudProviderPicker from './CloudProviderPicker.svelte'
import CloudProviderSetup from './CloudProviderSetup.svelte'
import OnboardingLanguagePicker from './OnboardingLanguagePicker.svelte'
import OnboardingStepShell from './OnboardingStepShell.svelte'
import OnboardingToggleCard from './OnboardingToggleCard.svelte'
import { cloudProviderPresets } from '$lib/settings'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function settle(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await Promise.resolve()
  }
  await tick()
  flushSync()
}

beforeEach(() => {
  for (const key of Object.keys(stubs.settingsMap)) delete stubs.settingsMap[key]
  stubs.getSetting = null
})

afterEach(async () => {
  if (mounted) {
    await unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

/** Tier 3 axe a11y test for `CloudProviderPicker.svelte`. */
describe('CloudProviderPicker a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(CloudProviderPicker, {
      target,
      props: { value: cloudProviderPresets[0].id, onChange: () => {} },
    })
    mounted = { target, instance }
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 axe a11y tests for `CloudProviderSetup.svelte`. */
describe('CloudProviderSetup a11y', () => {
  beforeEach(() => {
    stubs.settingsMap['ai.cloudProviderConfigs'] = '{}'
    stubs.getSetting = (id: string) => stubs.settingsMap[id] ?? ''
  })

  it('OpenAI provider state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'openai' } })
    mounted = { target, instance }
    await settle()
    await expectNoA11yViolations(target)
  })

  it('Custom provider (editable endpoint) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(CloudProviderSetup, { target, props: { providerId: 'custom' } })
    mounted = { target, instance }
    await settle()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 axe a11y tests for `OnboardingLanguagePicker.svelte`.
 *
 * Three states: closed on the `'system'` default, closed on an explicit pick, and open
 * with the menu rendered. The glyph is decorative (`aria-hidden`), so the trigger's
 * accessible name is the setting's own label; the open menu is Ark's listbox. Axe runs
 * in jsdom, so no contrast checks here (tier-1 scripts cover those).
 */
describe('OnboardingLanguagePicker a11y', () => {
  function mountPicker(): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // Portal the menu into `target` so axe sees the whole control in one tree, the way
    // the wizard portals it into its own overlay.
    mounted = { target, instance: mount(OnboardingLanguagePicker, { target, props: { portalContainer: target } }) }
    return target
  }

  beforeEach(() => {
    stubs.settingsMap['appearance.language'] = 'system'
    stubs.getSetting = (id: string) => stubs.settingsMap[id]
    _setSystemLocalesForTests({ ui: null, format: null })
  })

  afterEach(() => {
    document.body.innerHTML = ''
    _setSystemLocalesForTests({ ui: null, format: null })
  })

  it('closed, on the System default, has no a11y violations', async () => {
    const target = mountPicker()
    await settle(10)
    await expectNoA11yViolations(target)
  })

  it('closed, on an explicit pick, has no a11y violations', async () => {
    stubs.settingsMap['appearance.language'] = 'hu'
    const target = mountPicker()
    await settle(10)
    await expectNoA11yViolations(target)
  })

  it('open, with the language menu rendered, has no a11y violations', async () => {
    const target = mountPicker()
    await settle(10)
    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await settle(10)
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y test for `OnboardingStepShell.svelte`. Just a padded scroll container that
 * renders its children, so the assertion is structural: with a minimal child, axe finds
 * nothing wrong.
 */
describe('OnboardingStepShell a11y', () => {
  it('renders children without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const children = createRawSnippet(() => ({
      render: () => '<p>Test content</p>',
    }))
    const instance = mount(OnboardingStepShell, { target, props: { children } })
    mounted = { target, instance }
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `OnboardingToggleCard.svelte`, the bordered card `StepBeta` and
 * `StepOptional` both render. Two things have to survive a registry-backed mount: the
 * `<h3>` labelling the `<section>` through `titleId`, and the switch taking its
 * accessible name from the setting registry rather than from the visible title.
 */
describe('OnboardingToggleCard a11y', () => {
  beforeEach(() => {
    stubs.getSetting = (id: string) => stubs.settingsMap[id] ?? false
  })

  function mountCard(): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const children = createRawSnippet(() => ({
      render: () => '<p>What this sends, and what it never sends.</p>',
    }))
    mounted = {
      target,
      instance: mount(OnboardingToggleCard, {
        target,
        props: {
          titleId: 'toggle-analytics-title',
          title: 'Anonymous usage analytics',
          settingId: 'analytics.enabled',
          caption: 'On by default',
          children,
        },
      }),
    }
    return target
  }

  it('with the switch on has no a11y violations', async () => {
    stubs.settingsMap['analytics.enabled'] = true
    const target = mountCard()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with the switch off has no a11y violations', async () => {
    stubs.settingsMap['analytics.enabled'] = false
    const target = mountCard()
    await tick()
    await expectNoA11yViolations(target)
  })
})
