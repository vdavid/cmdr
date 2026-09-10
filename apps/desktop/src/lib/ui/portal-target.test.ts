/**
 * Where `Select` and `Combobox` mount their menus. Neither takes a say from the
 * caller: outside any dialog the menu lands in `document.body`, clear of every
 * ancestor's `overflow`, mask, and stacking context; inside a `ModalDialog` it
 * lands in the dialog's overlay, which keeps it above the scrim and inside the
 * focus trap while still escaping the panel's clip.
 *
 * jsdom has no layout, so this pins the DOM placement only. Whether the placed
 * menu actually paints on top is `viewer-media.spec.ts`'s hit test.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import Select from './Select.svelte'
import Combobox from './Combobox.svelte'
import PortalTargetFixture from '../../../test/fixtures/portal-target-fixture.svelte'

// Avoid Tauri IPC side-effects from `ModalDialog`'s notifyDialogOpened / notifyDialogClosed.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const cleanups: (() => void)[] = []

afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup()
})

function mountInto<Props extends Record<string, unknown>>(
  component: Parameters<typeof mount<Props, Record<string, never>>>[0],
  props: Props,
): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(component, { target, props })
  cleanups.push(() => {
    void unmount(instance)
    target.remove()
  })
  return target
}

/** Ark's `Portal` mounts its children on the tick after its own effect runs. */
async function settle(): Promise<void> {
  await tick()
  await tick()
}

/** The element a menu's positioner was mounted into (the positioner is the content's parent). */
function menuHost(contentSelector: string): Element | null {
  return document.querySelector(contentSelector)?.parentElement?.parentElement ?? null
}

const selectProps = {
  items: [
    { value: 'a', label: 'A' },
    { value: 'b', label: 'B' },
  ],
  value: 'a',
  onChange: () => {},
  ariaLabel: 'Pick one',
}

const comboboxProps = {
  items: [{ value: 'x', label: 'X' }],
  inputValue: '',
  onInputValueChange: () => {},
  ariaLabel: 'Type one',
}

describe('floating menu placement', () => {
  it('mounts a Select menu in document.body when no dialog hosts it', async () => {
    const target = mountInto(Select, selectProps)
    await settle()

    expect(menuHost('.select-content')).toBe(document.body)
    expect(target.querySelector('.select-content')).toBeNull()
  })

  it('mounts a Combobox menu in document.body when no dialog hosts it', async () => {
    const target = mountInto(Combobox, comboboxProps)
    await settle()

    expect(menuHost('.combobox-content')).toBe(document.body)
    expect(target.querySelector('.combobox-content')).toBeNull()
  })

  it("mounts both menus in a ModalDialog's overlay, outside the clipping panel", async () => {
    mountInto(PortalTargetFixture, {})
    await settle()

    const overlay = document.querySelector('.modal-overlay')
    expect(overlay).not.toBeNull()
    expect(menuHost('.select-content')).toBe(overlay)
    expect(menuHost('.combobox-content')).toBe(overlay)
    expect(document.querySelector('.modal-dialog .select-content')).toBeNull()
    expect(document.querySelector('.modal-dialog .combobox-content')).toBeNull()
  })
})
