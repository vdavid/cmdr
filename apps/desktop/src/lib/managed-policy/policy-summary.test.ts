import { describe, it, expect, afterEach } from 'vitest'
import type { ManagedPolicyView } from '$lib/ipc/bindings'
import { setLocale, _setCatalogForTests } from '$lib/intl/messages.svelte'
import { UNMANAGED } from './managed-policy.svelte'
import { managedPolicySummary } from './policy-summary'

const TEST_LANG = 'zz'

afterEach(() => {
  setLocale(null)
  _setCatalogForTests(TEST_LANG, null)
})

function view(overrides: Partial<ManagedPolicyView>): ManagedPolicyView {
  return { ...UNMANAGED, managed: true, ...overrides }
}

describe('managedPolicySummary', () => {
  it('lists nothing when nothing is managed', () => {
    expect(managedPolicySummary(UNMANAGED)).toEqual([])
  })

  it('names each restriction in a fixed order, and only the restricted ones', () => {
    const lines = managedPolicySummary(
      view({
        usageStatsDisabled: true,
        reportsDisabled: true,
        updates: { kind: 'disabled' },
        ai: { mode: 'off', allowedCloudHosts: null },
      }),
    )
    expect(lines).toEqual([
      { label: 'Usage stats', value: 'Off' },
      { label: 'Crash and error reports', value: 'Off' },
      { label: 'Updates', value: 'Off' },
      { label: 'AI', value: 'Off' },
    ])
  })

  // Each row's "Off" is its own key, so a language can agree the word with that row's label
  // (es "Desactivadas" beside a feminine plural, "Desactivado" beside a masculine singular).
  it('words each row’s Off from its own key', () => {
    _setCatalogForTests(TEST_LANG, {
      'settings.managed.summary.usageStatsOff': 'usage-off',
      'settings.managed.summary.reportsOff': 'reports-off',
      'settings.managed.summary.updatesOff': 'updates-off',
      'settings.managed.summary.aiOff': 'ai-off',
    })
    setLocale(TEST_LANG)
    const values = managedPolicySummary(
      view({
        usageStatsDisabled: true,
        reportsDisabled: true,
        updates: { kind: 'disabled' },
        ai: { mode: 'off', allowedCloudHosts: null },
      }),
    ).map((line) => line.value)
    expect(values).toEqual(['usage-off', 'reports-off', 'updates-off', 'ai-off'])
  })

  it('leaves out what the policy doesn’t touch', () => {
    expect(managedPolicySummary(view({ reportsDisabled: true }))).toEqual([
      { label: 'Crash and error reports', value: 'Off' },
    ])
  })

  it('words a ceiling, manual-only checks, and both together', () => {
    const ceiling = managedPolicySummary(view({ updates: { kind: 'enabled', automaticChecks: true, ceiling: '0.52' } }))
    const manual = managedPolicySummary(view({ updates: { kind: 'enabled', automaticChecks: false, ceiling: null } }))
    const both = managedPolicySummary(view({ updates: { kind: 'enabled', automaticChecks: false, ceiling: '0.52.3' } }))
    expect(ceiling).toEqual([{ label: 'Updates', value: 'Up to 0.52' }])
    expect(manual).toEqual([{ label: 'Updates', value: 'Checked by hand only' }])
    expect(both).toEqual([{ label: 'Updates', value: 'Up to 0.52.3, checked by hand only' }])
  })

  it('words on-device-only AI and an allowed host list', () => {
    const local = managedPolicySummary(view({ ai: { mode: 'localOnly', allowedCloudHosts: null } }))
    const hosts = managedPolicySummary(
      view({ ai: { mode: 'allowed', allowedCloudHosts: ['api.openai.com', '*.openai.azure.com'] } }),
    )
    expect(local).toEqual([{ label: 'AI', value: 'On-device only' }])
    expect(hosts).toEqual([{ label: 'AI', value: 'Cloud AI only through api.openai.com and *.openai.azure.com' }])
  })
})
