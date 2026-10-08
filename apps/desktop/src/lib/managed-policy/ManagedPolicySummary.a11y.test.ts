/**
 * Tier-3 a11y test for `ManagedPolicySummary.svelte`: the labelled card and its description list,
 * with every kind of line a policy can produce.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { ManagedPolicyView } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const managed: ManagedPolicyView = {
  managed: true,
  usageStatsDisabled: true,
  reportsDisabled: true,
  updates: { kind: 'enabled', automaticChecks: false, ceiling: '0.52' },
  ai: { mode: 'allowed', allowedCloudHosts: ['api.openai.com', 'localhost'] },
  lockedSettings: [],
}

vi.mock('./managed-policy.svelte', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./managed-policy.svelte')>()),
  getManagedPolicyView: () => managed,
}))

import ManagedPolicySummary from './ManagedPolicySummary.svelte'

afterEach(() => {
  document.body.innerHTML = ''
})

describe('ManagedPolicySummary a11y', () => {
  it('has no violations with every area listed', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ManagedPolicySummary, { target })
    await tick()

    expect(target.querySelectorAll('dt')).toHaveLength(4)
    await expectNoA11yViolations(target)
  })
})
