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

describe('EncodingPicker', () => {
  it('renders every choice grouped by Unicode and Western', async () => {
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8' })
    await tick()

    const select = target.querySelector('select.encoding-picker')
    expect(select).not.toBeNull()
    const optgroups = target.querySelectorAll('optgroup')
    expect(optgroups).toHaveLength(2)
    const labels = Array.from(optgroups).map((o) => o.getAttribute('label'))
    expect(labels).toContain('Unicode')
    expect(labels).toContain('Western')

    const options = target.querySelectorAll('option')
    expect(options.length).toBe(allChoices.length)

    void unmount(instance)
  })

  it('marks the detected encoding with "(Detected)"', async () => {
    const { target, instance } = mountPicker({ value: 'windows1252', detected: 'windows1252' })
    await tick()

    const detectedOption = Array.from(target.querySelectorAll('option')).find(
      (o) => o.getAttribute('value') === 'windows1252',
    )
    expect(detectedOption?.textContent).toContain('(Detected)')
    const undetectedOption = Array.from(target.querySelectorAll('option')).find(
      (o) => o.getAttribute('value') === 'utf8',
    )
    expect(undetectedOption?.textContent).not.toContain('(Detected)')

    void unmount(instance)
  })

  it('calls onChange with the picked encoding', async () => {
    const onChange = vi.fn()
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8', onChange })
    await tick()

    const select = target.querySelector('select.encoding-picker') as HTMLSelectElement
    select.value = 'utf16Le'
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()

    expect(onChange).toHaveBeenCalledTimes(1)
    expect(onChange).toHaveBeenCalledWith('utf16Le')

    void unmount(instance)
  })

  it('reflects the disabled prop', async () => {
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8', disabled: true })
    await tick()

    const select = target.querySelector('select.encoding-picker') as HTMLSelectElement
    expect(select.disabled).toBe(true)

    void unmount(instance)
  })

  it('preserves the backend-supplied order within each group', async () => {
    const { target, instance } = mountPicker({ value: 'utf8', detected: 'utf8' })
    await tick()

    const unicodeOptions = Array.from(target.querySelectorAll('optgroup[label="Unicode"] option')).map(
      (o) => (o as HTMLOptionElement).value,
    )
    expect(unicodeOptions).toEqual(['utf8', 'utf8WithBom', 'utf16Le', 'utf16Be'])

    void unmount(instance)
  })
})
