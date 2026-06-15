import { describe, it, expect, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import ViewModePicker from './ViewModePicker.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountPicker(kind: 'text' | 'image' | 'pdf' = 'text', lastMediaKind: 'text' | 'image' | 'pdf' | null = null) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(ViewModePicker, {
    target,
    props: { kind, lastMediaKind, onViewAsText: () => {}, onViewAsMedia: () => {} },
  })
  return { target, instance }
}

describe('ViewModePicker accessibility', () => {
  it('has no a11y violations on the closed (disabled) picker', async () => {
    const { target, instance } = mountPicker()
    await tick()
    await expectNoA11yViolations(target)
    void unmount(instance)
  })

  it('exposes aria-label on the trigger so AT can identify the picker', async () => {
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })

  it('surfaces its disabled state to AT for a genuine text file', async () => {
    // A genuine text file (no remembered media kind) has nothing to switch to, so
    // the picker is disabled. Pin the contract so a future "make it look enabled"
    // refactor can't silently drop the disabled announcement. Ark reflects it as
    // `data-disabled` plus `disabled` on the trigger button.
    const { target, instance } = mountPicker()
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger).not.toBeNull()
    expect(trigger?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('has no a11y violations on the enabled reverse-switch picker (media file read as text)', async () => {
    const { target, instance } = mountPicker('text', 'image')
    await tick()
    await expectNoA11yViolations(target)
    void unmount(instance)
  })

  it('uses the listbox combobox pattern for keyboard navigation', async () => {
    // The Ark `Select` gives a `role="combobox"` trigger and a `role="listbox"`
    // popover with full keyboard support out of the box. Pin that the picker
    // stays on the accessible widget rather than a bare button.
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('[role="combobox"]')).not.toBeNull()
    expect(target.querySelector('[role="listbox"]')).not.toBeNull()
    const option = target.querySelector('[data-part="item"][data-value="text"]')
    expect(option?.textContent).toContain('Text')

    void unmount(instance)
  })
})
