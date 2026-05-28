import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('ViewModePicker accessibility', () => {
  it('exposes aria-label so AT can identify the picker', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, {
      target,
      props: {
        value: 'text',
        onChange: () => {},
      },
    })
    await tick()

    const select = target.querySelector('select.view-mode-picker')
    expect(select?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })

  it('surfaces its disabled state to AT via the disabled attribute', async () => {
    // Only one mode ships today; the picker is disabled. AT announces the
    // disabled state via the native <select disabled> attribute, no extra
    // ARIA needed. Pin the contract so a future "make it look enabled"
    // refactor can't silently drop the disabled announcement.
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, {
      target,
      props: {
        value: 'text',
        onChange: () => {},
      },
    })
    await tick()

    const select = target.querySelector('select.view-mode-picker') as HTMLSelectElement | null
    expect(select).not.toBeNull()
    expect(select?.disabled).toBe(true)

    void unmount(instance)
  })

  it('uses native <select> + <option> for keyboard navigation', async () => {
    // Native <select> handles Tab focus, arrow-key option change, and Enter
    // commit out of the box. The test pins that the picker stays on the
    // native primitive rather than a custom widget that would need explicit
    // ARIA roles + keyboard handlers (and would lose AT support).
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, {
      target,
      props: {
        value: 'text',
        onChange: () => {},
      },
    })
    await tick()

    expect(target.querySelector('select')).not.toBeNull()
    expect(target.querySelectorAll('option').length).toBeGreaterThan(0)
    const option = target.querySelector('option[value="text"]')
    expect(option?.textContent?.trim()).toBe('Text')

    void unmount(instance)
  })
})
