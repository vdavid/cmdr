import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'
import type { ViewerContentKind } from '$lib/ipc/bindings'

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker(kind: ViewerContentKind, onViewAsText?: () => void) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewModePicker, { target, props: { kind, onViewAsText } })
  return { target, instance }
}

describe('ViewModePicker', () => {
  it('renders a single, disabled "Text" option for a text file', async () => {
    const { target, instance } = mountPicker('text')
    await tick()

    const options = target.querySelectorAll('[data-part="item"]')
    expect(options).toHaveLength(1)
    expect(options[0].getAttribute('data-value')).toBe('text')
    expect(options[0].textContent).toContain('Text')
    expect(target.querySelector<HTMLButtonElement>('.select-trigger')?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('shows the detected kind on the trigger for media', async () => {
    const { target, instance } = mountPicker('image')
    await tick()

    expect(target.querySelector('[data-part="value-text"]')?.textContent).toContain('Image')

    void unmount(instance)
  })

  it('offers "View as text" for a media file and is enabled', async () => {
    const { target, instance } = mountPicker('pdf')
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger?.hasAttribute('data-disabled')).toBe(false)
    expect(trigger?.textContent).toContain('PDF')
    const values = Array.from(target.querySelectorAll('[data-part="item"]')).map((o) => o.getAttribute('data-value'))
    expect(values).toContain('viewAsText')

    void unmount(instance)
  })

  it('calls onViewAsText when "View as text" is picked', async () => {
    const onViewAsText = vi.fn()
    const { target, instance } = mountPicker('image', onViewAsText)
    await tick()

    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()
    const item = Array.from(target.querySelectorAll<HTMLElement>('[data-part="item"]')).find(
      (o) => o.getAttribute('data-value') === 'viewAsText',
    )
    item?.click()
    await tick()

    expect(onViewAsText).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })

  it('exposes an aria-label so AT can identify the picker', async () => {
    const { target, instance } = mountPicker('text')
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })
})
