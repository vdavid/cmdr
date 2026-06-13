import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import EncodingPicker from './EncodingPicker.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { EncodingChoice } from '$lib/ipc/bindings'

const choices: EncodingChoice[] = [
  { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
  { encoding: 'utf16Le', label: 'UTF-16 LE', group: 'unicode' },
  { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker() {
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
  return { target, instance }
}

describe('EncodingPicker accessibility', () => {
  it('has no a11y violations on the closed picker', async () => {
    const { target, instance } = mountPicker()
    await tick()
    await expectNoA11yViolations(target)
    void unmount(instance)
  })

  it('exposes an aria-label on the trigger so AT can identify the picker', async () => {
    const { target, instance } = mountPicker()
    await tick()

    const trigger = target.querySelector('.select-trigger')
    expect(trigger?.getAttribute('aria-label')).toBe('Encoding')

    void unmount(instance)
  })

  it('uses the listbox combobox pattern with grouped options', async () => {
    // The Ark `Select` gives a `role="combobox"` trigger and a `role="listbox"`
    // popover whose options are bucketed under `role="group"` headings, with
    // full keyboard support (Tab focus, arrow-key option change, Enter commit).
    // Pin that the picker renders the accessible widget, not a bare button.
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('[role="combobox"]')).not.toBeNull()
    expect(target.querySelector('[role="listbox"]')).not.toBeNull()
    expect(target.querySelectorAll('[data-part="item-group-label"]').length).toBeGreaterThan(0)

    void unmount(instance)
  })
})
