import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewModePicker, { target, props: { value: 'text' } })
  return { target, instance }
}

describe('ViewModePicker', () => {
  it('renders a single "Text" option', async () => {
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('.select-trigger')).not.toBeNull()
    const options = target.querySelectorAll('[data-part="item"]')
    expect(options).toHaveLength(1)
    expect(options[0].getAttribute('data-value')).toBe('text')
    expect(options[0].textContent).toContain('Text')

    void unmount(instance)
  })

  it('shows "Text" as the selected value on the trigger', async () => {
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('[data-part="value-text"]')?.textContent).toContain('Text')

    void unmount(instance)
  })

  it('is disabled because no other modes are available yet', async () => {
    const { target, instance } = mountPicker()
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('exposes an aria-label so AT can identify the picker', async () => {
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })
})
