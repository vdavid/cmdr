import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('ViewModePicker', () => {
  it('renders a single "Text" option', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, { target, props: { value: 'text' } })
    await tick()

    const select = target.querySelector('select.view-mode-picker')
    expect(select).not.toBeNull()
    const options = target.querySelectorAll('option')
    expect(options).toHaveLength(1)
    const first = options[0]
    expect(first.textContent).toBe('Text')
    expect(first.getAttribute('value')).toBe('text')

    void unmount(instance)
  })

  it('is disabled because no other modes are available yet', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, { target, props: { value: 'text' } })
    await tick()

    const select = target.querySelector('select.view-mode-picker') as HTMLSelectElement
    expect(select.disabled).toBe(true)

    void unmount(instance)
  })

  it('exposes an aria-label so AT can identify the picker', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, { target, props: { value: 'text' } })
    await tick()

    const select = target.querySelector('select.view-mode-picker')
    expect(select?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })
})
