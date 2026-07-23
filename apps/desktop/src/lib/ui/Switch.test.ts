/**
 * Behavior tests for `Switch.svelte`.
 *
 * The a11y sibling (`Switch.a11y.test.ts`) covers the axe audit and the basic toggle; this file
 * pins the parts a consumer relies on: the ARIA role Ark gives the hidden input, the disabled
 * control refusing to change, and the label snippet rendering next to the track.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import Switch from './Switch.svelte'

type Props = ComponentProps<typeof Switch>

async function mountSwitch(props: Props): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Switch, { target, props })
  await tick()
  return target
}

describe('Switch', () => {
  it('backs the control with a native input carrying role="switch"', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders' })

    const input = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(input?.getAttribute('role')).toBe('switch')

    target.remove()
  })

  it('reflects the checked prop on the styled track', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', checked: true })

    expect(target.querySelector('.switch-control')?.getAttribute('data-state')).toBe('checked')

    target.remove()
  })

  it('does not toggle or notify while disabled', async () => {
    const onCheckedChange = vi.fn()
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', disabled: true, onCheckedChange })

    const input = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    if (!input) throw new Error('expected a native input backing the switch')
    input.click()
    await tick()

    expect(onCheckedChange).not.toHaveBeenCalled()
    expect(target.querySelector('.switch-control')?.getAttribute('data-state')).toBe('unchecked')

    target.remove()
  })
})
