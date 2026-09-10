import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import EncodingPicker from './EncodingPicker.svelte'
import type { EncodingChoice, FileEncoding } from '$lib/ipc/bindings'

const allChoices: EncodingChoice[] = [
  { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
  { encoding: 'utf8WithBom', label: 'UTF-8 with BOM', group: 'unicode' },
  { encoding: 'utf16Le', label: 'UTF-16 LE', group: 'unicode' },
  { encoding: 'utf16Be', label: 'UTF-16 BE', group: 'unicode' },
  { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
  { encoding: 'iso8859_1', label: 'Western (ISO-8859-1)', group: 'western' },
  { encoding: 'macRoman', label: 'Western (Mac Roman)', group: 'western' },
  { encoding: 'usAscii', label: 'US-ASCII', group: 'western' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker(opts: {
  value: FileEncoding
  detected: FileEncoding
  onChange?: (encoding: FileEncoding) => void
  disabled?: boolean
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(EncodingPicker, {
    target,
    props: {
      value: opts.value,
      detected: opts.detected,
      options: allChoices,
      disabled: opts.disabled,
      onChange: opts.onChange ?? (() => {}),
    },
  })
  return { target, instance }
}

/** Ark's `Portal` mounts the menu on the tick after its own effect runs, so wait out both. */
async function settle(): Promise<void> {
  await tick()
  await tick()
}

/**
 * The Ark `Select` renders every option in the DOM even while closed. The menu portals to
 * `document.body`, so its parts are found there, never under the mount target.
 */
function optionEls(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]'))
}

function optionByValue(value: string): HTMLElement | undefined {
  return optionEls().find((el) => el.getAttribute('data-value') === value)
}

function groupLabels(): string[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-part="item-group-label"]')).map((el) =>
    el.textContent.trim(),
  )
}

describe('EncodingPicker', () => {
  it('renders every choice grouped by Unicode and Western', async () => {
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8' })
    await settle()

    expect(target.querySelector('.select-trigger')).not.toBeNull()
    expect(groupLabels()).toContain('Unicode')
    expect(groupLabels()).toContain('Western')
    expect(optionEls()).toHaveLength(allChoices.length)

    void unmount(instance)
  })

  it('marks the detected encoding with "(Detected)"', async () => {
    const { instance } = mountPicker({ value: 'windows1252', detected: 'windows1252' })
    await settle()

    expect(optionByValue('windows1252')?.textContent).toContain('(Detected)')
    expect(optionByValue('utf8')?.textContent).not.toContain('(Detected)')

    void unmount(instance)
  })

  it('calls onChange with the picked encoding', async () => {
    const onChange = vi.fn()
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8', onChange })
    await settle()

    // Open the listbox before picking: Ark only routes selection through the
    // interaction machinery while the content is open (closed content is hidden).
    target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
    await tick()
    optionByValue('utf16Le')?.click()
    await tick()

    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange).toHaveBeenCalledWith('utf16Le')

    void unmount(instance)
  })

  it('reflects the disabled prop on the trigger', async () => {
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8', disabled: true })
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger).not.toBeNull()
    expect(trigger?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('preserves the backend-supplied order within each group', async () => {
    const { instance } = mountPicker({ value: 'utf8', detected: 'utf8' })
    await settle()

    const unicodeGroup = Array.from(document.querySelectorAll<HTMLElement>('[data-part="item-group"]')).find(
      (g) => (g.querySelector('[data-part="item-group-label"]')?.textContent ?? '').trim() === 'Unicode',
    )
    const unicodeValues = Array.from(unicodeGroup?.querySelectorAll<HTMLElement>('[data-part="item"]') ?? []).map(
      (el) => el.getAttribute('data-value'),
    )
    expect(unicodeValues).toEqual(['utf8', 'utf8WithBom', 'utf16Le', 'utf16Be'])

    void unmount(instance)
  })

  it('shows the currently selected encoding on the trigger', async () => {
    const { target, instance } = mountPicker({ value: 'utf16Le', detected: 'utf8' })
    await tick()

    const valueText = target.querySelector('[data-part="value-text"]')
    expect(valueText?.textContent).toContain('UTF-16 LE')

    void unmount(instance)
  })
})
