import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'
import type { ViewerContentKind } from '$lib/ipc/bindings'

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker(props: {
  kind: ViewerContentKind
  lastMediaKind?: ViewerContentKind | null
  onViewAsText?: () => void
  onViewAsMedia?: () => void
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewModePicker, { target, props: { lastMediaKind: null, ...props } })
  return { target, instance }
}

function itemValues(target: HTMLElement): (string | null)[] {
  return Array.from(target.querySelectorAll('[data-part="item"]')).map((o) => o.getAttribute('data-value'))
}

describe('ViewModePicker', () => {
  it('renders a single, disabled "Text" option for a genuine text file', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: null })
    await tick()

    const options = target.querySelectorAll('[data-part="item"]')
    expect(options).toHaveLength(1)
    expect(options[0].getAttribute('data-value')).toBe('text')
    expect(options[0].textContent).toContain('Text')
    expect(target.querySelector<HTMLButtonElement>('.select-trigger')?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('shows the detected kind on the trigger for media', async () => {
    const { target, instance } = mountPicker({ kind: 'image' })
    await tick()

    expect(target.querySelector('[data-part="value-text"]')?.textContent).toContain('Image')

    void unmount(instance)
  })

  it('offers "View as text" for a media file and is enabled', async () => {
    const { target, instance } = mountPicker({ kind: 'pdf' })
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger?.hasAttribute('data-disabled')).toBe(false)
    expect(trigger?.textContent).toContain('PDF')
    expect(itemValues(target)).toEqual(['pdf', 'viewAsText'])

    void unmount(instance)
  })

  it('offers the reverse "View as image" while reading a media file as text', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: 'image' })
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    // Not disabled: there's a real switch-back available.
    expect(trigger?.hasAttribute('data-disabled')).toBe(false)
    expect(trigger?.textContent).toContain('Text')
    expect(itemValues(target)).toEqual(['text', 'viewAsMedia'])
    const reverse = Array.from(target.querySelectorAll<HTMLElement>('[data-part="item"]')).find(
      (o) => o.getAttribute('data-value') === 'viewAsMedia',
    )
    expect(reverse?.textContent).toContain('View as image')

    void unmount(instance)
  })

  it('offers the reverse "View as PDF" while reading a PDF as text (PDF stays uppercase)', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: 'pdf' })
    await tick()

    const reverse = Array.from(target.querySelectorAll<HTMLElement>('[data-part="item"]')).find(
      (o) => o.getAttribute('data-value') === 'viewAsMedia',
    )
    expect(reverse?.textContent).toContain('View as PDF')

    void unmount(instance)
  })

  it('calls onViewAsText when "View as text" is picked', async () => {
    const onViewAsText = vi.fn()
    const { target, instance } = mountPicker({ kind: 'image', onViewAsText })
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

  it('calls onViewAsMedia when "View as image" is picked from the text view', async () => {
    const onViewAsMedia = vi.fn()
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: 'image', onViewAsMedia })
    await tick()

    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()
    const item = Array.from(target.querySelectorAll<HTMLElement>('[data-part="item"]')).find(
      (o) => o.getAttribute('data-value') === 'viewAsMedia',
    )
    item?.click()
    await tick()

    expect(onViewAsMedia).toHaveBeenCalledTimes(1)

    void unmount(instance)
  })

  it('exposes an aria-label so AT can identify the picker', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: null })
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })
})
