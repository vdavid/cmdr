import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'
import type { ViewerContentKind } from '$lib/ipc/bindings'
import type { ViewerDisplayMode } from './viewer-view-mode'

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker(props: {
  kind: ViewerContentKind
  mode?: ViewerDisplayMode
  lastMediaKind?: ViewerContentKind | null
  onModeChange?: (mode: ViewerDisplayMode) => void
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewModePicker, {
    target,
    props: { lastMediaKind: null, mode: props.kind === 'text' ? 'text' : 'media', onModeChange: () => {}, ...props },
  })
  return { target, instance }
}

/** Ark's `Portal` mounts the menu on the tick after its own effect runs, so wait out both. */
async function settle(): Promise<void> {
  await tick()
  await tick()
}

/** The menu's rows. It portals to `document.body`, so they're never under the mount target. */
function items(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]'))
}

function itemValues(): (string | null)[] {
  return items().map((o) => o.getAttribute('data-value'))
}

describe('ViewModePicker', () => {
  it('offers text, binary, and hex for a genuine text file', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: null })
    await settle()

    const options = items()
    expect(options).toHaveLength(3)
    expect(options[0].getAttribute('data-value')).toBe('text')
    expect(options[0].textContent).toContain('Text (1)')
    expect(itemValues()).toEqual(['text', 'binary', 'hex'])
    expect(target.querySelector<HTMLButtonElement>('.select-trigger')?.hasAttribute('data-disabled')).toBe(false)

    void unmount(instance)
  })

  it('shows the detected kind on the trigger for media', async () => {
    const { target, instance } = mountPicker({ kind: 'image' })
    await tick()

    expect(target.querySelector('[data-part="value-text"]')?.textContent).toContain('Image')

    void unmount(instance)
  })

  it('offers all raw modes for a media file', async () => {
    const { target, instance } = mountPicker({ kind: 'pdf' })
    await settle()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger?.hasAttribute('data-disabled')).toBe(false)
    expect(trigger?.textContent).toContain('PDF')
    expect(itemValues()).toEqual(['text', 'binary', 'hex', 'media'])
    expect(items().find((o) => o.getAttribute('data-value') === 'media')?.textContent).toContain('PDF (0)')

    void unmount(instance)
  })

  it('offers the reverse "View as image" while reading a media file as text', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: 'image' })
    await settle()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    // Not disabled: there's a real switch-back available.
    expect(trigger?.hasAttribute('data-disabled')).toBe(false)
    expect(trigger?.textContent).toContain('Text')
    expect(itemValues()).toEqual(['text', 'binary', 'hex', 'media'])
    const reverse = items().find((o) => o.getAttribute('data-value') === 'media')
    expect(reverse?.textContent).toContain('View as image (0)')

    void unmount(instance)
  })

  it('offers the reverse "View as PDF" while reading a PDF as text (PDF stays uppercase)', async () => {
    const { instance } = mountPicker({ kind: 'text', lastMediaKind: 'pdf' })
    await settle()

    const reverse = items().find((o) => o.getAttribute('data-value') === 'media')
    expect(reverse?.textContent).toContain('View as PDF (0)')

    void unmount(instance)
  })

  it('reports the selected mode', async () => {
    const onModeChange = vi.fn()
    const { target, instance } = mountPicker({ kind: 'image', onModeChange })
    await settle()

    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()
    const item = items().find((o) => o.getAttribute('data-value') === 'hex')
    item?.click()
    await tick()

    expect(onModeChange).toHaveBeenCalledWith('hex')

    void unmount(instance)
  })

  it('can return to rendered media from text', async () => {
    const onModeChange = vi.fn()
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: 'image', onModeChange })
    await settle()

    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()
    const item = items().find((o) => o.getAttribute('data-value') === 'media')
    item?.click()
    await tick()

    expect(onModeChange).toHaveBeenCalledWith('media')

    void unmount(instance)
  })

  it('exposes an aria-label so AT can identify the picker', async () => {
    const { target, instance } = mountPicker({ kind: 'text', lastMediaKind: null })
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })
})
