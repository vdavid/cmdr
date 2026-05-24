/**
 * Tier 3 axe a11y test for `CloudProviderPicker.svelte` (M3).
 */

import { describe, it, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import CloudProviderPicker from './CloudProviderPicker.svelte'
import { cloudProviderPresets } from '$lib/settings'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

beforeEach(() => {})
afterEach(() => {
  if (mounted) {
    unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

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
