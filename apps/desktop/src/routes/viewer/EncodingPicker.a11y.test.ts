import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import EncodingPicker from './EncodingPicker.svelte'
import type { EncodingChoice } from '$lib/ipc/bindings'

const choices: EncodingChoice[] = [
  { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
  { encoding: 'utf16Le', label: 'UTF-16 LE', group: 'unicode' },
  { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('EncodingPicker accessibility', () => {
  it('exposes an aria-label so AT can identify the picker', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(EncodingPicker, {
      target,
      props: {
        value: 'utf8',
        detected: 'utf8',
        options: choices,
        onChange: () => {},
      },
    })
    await tick()

    const select = target.querySelector('select.encoding-picker')
    expect(select?.getAttribute('aria-label')).toBe('Encoding')

    void unmount(instance)
  })

  it('uses native <select> + <optgroup> for keyboard navigation', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(EncodingPicker, {
      target,
      props: {
        value: 'utf8',
        detected: 'utf8',
        options: choices,
        onChange: () => {},
      },
    })
    await tick()

    // Native <select> handles Tab focus, arrow-key option change, and Enter
    // commit out of the box. Re-implementing these via a custom widget would
    // need explicit ARIA roles + keyboard handlers; the test pins that we
    // chose the native primitive.
    expect(target.querySelector('select')).not.toBeNull()
    expect(target.querySelectorAll('optgroup').length).toBeGreaterThan(0)

    void unmount(instance)
  })
})
