/**
 * Behavior tests for `MaskInput.svelte`: a counter token gets a marker, ArrowDown with the caret at
 * the token (or a click on its marker) opens the counter editor, an edit rewrites the token in
 * minimal form, and leaving the editor puts the caret right after the token. Enter and Escape in the
 * editor stay in it, so the sheet around it neither starts nor closes.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

interface Mounted {
  input: HTMLInputElement
  value: () => string
  /** Keys that reached the sheet around the field. */
  outerKeys: string[]
}

async function mountField(initial: string): Promise<Mounted> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const outerKeys: string[] = []
  const state = { value: initial }
  const harness = mount(MaskInputFixture, {
    target,
    props: {
      initial,
      onValue: (next: string) => {
        state.value = next
      },
      onOuterKey: (key: string) => outerKeys.push(key),
    },
  })
  cleanup = () => {
    void unmount(harness)
  }
  await settle()
  const input = target.querySelector<HTMLInputElement>('input')
  if (!input) throw new Error('no mask field')
  return { input, value: () => state.value, outerKeys }
}

function key(el: Element, k: string): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
}

function editorFields(): HTMLInputElement[] {
  return [...document.querySelectorAll<HTMLInputElement>('.ui-popover input')]
}

function type(field: HTMLInputElement, text: string): void {
  field.value = text
  field.dispatchEvent(new Event('input', { bubbles: true }))
}

async function openWithArrowDown(m: Mounted, caret: number): Promise<void> {
  m.input.focus()
  m.input.setSelectionRange(caret, caret)
  key(m.input, 'ArrowDown')
  await settle()
}

describe('MaskInput', () => {
  it('draws one marker per counter token, labeled for what it does', async () => {
    await mountField('IMG_[C] [N] [C:3]')
    const markers = document.querySelectorAll('button[aria-label="Edit counter"]')
    expect(markers).toHaveLength(2)
  })

  it('opens the editor with ArrowDown when the caret is right after a counter token', async () => {
    const m = await mountField('IMG_[C10]')
    await openWithArrowDown(m, 9)
    const popover = document.querySelector('.ui-popover')
    expect(popover?.getAttribute('aria-label')).toBe('Counter')
    expect(editorFields().map((f) => f.value)).toEqual(['10', '1', '1'])
    expect(document.activeElement).toBe(editorFields()[0])
  })

  it('leaves ArrowDown alone when the caret is away from any counter', async () => {
    const m = await mountField('IMG_[C] [N]')
    m.input.focus()
    m.input.setSelectionRange(2, 2)
    const event = new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })
    m.input.dispatchEvent(event)
    await settle()
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(event.defaultPrevented).toBe(false)
    expect(m.outerKeys).toEqual(['ArrowDown'])
  })

  it('opens the editor for the token whose marker was clicked', async () => {
    await mountField('[C] x [C+5]')
    const markers = document.querySelectorAll<HTMLButtonElement>('button[aria-label="Edit counter"]')
    markers[1].click()
    await settle()
    expect(editorFields().map((f) => f.value)).toEqual(['1', '5', '1'])
  })

  it('rewrites the token on every edit, in minimal form', async () => {
    const m = await mountField('IMG_[C] [N]')
    await openWithArrowDown(m, 7)
    const [start, step, digits] = editorFields()
    type(digits, '3')
    await settle()
    expect(m.value()).toBe('IMG_[C:3] [N]')
    type(start, '10')
    await settle()
    type(step, '-2')
    await settle()
    expect(m.value()).toBe('IMG_[C10-2:3] [N]')
    expect(document.querySelector('.ui-popover')?.textContent).toContain('Counts 10, 8, 6…, shown as 010, 008, 006')
    type(start, '1')
    await settle()
    type(step, '')
    await settle()
    type(digits, '1')
    await settle()
    expect(m.value()).toBe('IMG_[C] [N]')
  })

  it('waits on a half-typed number and clamps one past the limits', async () => {
    const m = await mountField('[C]')
    await openWithArrowDown(m, 3)
    const [, step, digits] = editorFields()
    type(step, '-')
    await settle()
    expect(m.value()).toBe('[C]')
    type(digits, '500')
    await settle()
    expect(m.value()).toBe('[C:64]')
    const [start] = editorFields()
    type(start, '-5')
    await settle()
    expect(m.value()).toBe('[C-5+1:64]')
    type(start, '-9999999')
    await settle()
    expect(m.value()).toBe('[C-1000000+1:64]')
  })

  it('Escape closes the editor, not the sheet, and puts the caret after the token', async () => {
    const m = await mountField('a[C]b')
    await openWithArrowDown(m, 2)
    const [, step] = editorFields()
    type(step, '2')
    await settle()
    key(step, 'Escape')
    await settle()
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(m.outerKeys).toEqual([])
    expect(document.activeElement).toBe(m.input)
    expect(m.input.selectionStart).toBe(6)
    expect(m.value()).toBe('a[C+2]b')
  })

  it('Enter closes the editor without reaching the sheet', async () => {
    const m = await mountField('[C] x')
    await openWithArrowDown(m, 3)
    key(editorFields()[2], 'Enter')
    await settle()
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(m.outerKeys).toEqual([])
    expect(document.activeElement).toBe(m.input)
    expect(m.input.selectionStart).toBe(3)
  })

  it('ArrowDown and ArrowUp walk the fields, and ArrowUp from the first one leaves', async () => {
    const m = await mountField('[C:2]')
    await openWithArrowDown(m, 5)
    const [start, step] = editorFields()
    key(start, 'ArrowDown')
    expect(document.activeElement).toBe(step)
    key(step, 'ArrowUp')
    expect(document.activeElement).toBe(start)
    key(start, 'ArrowUp')
    await settle()
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(document.activeElement).toBe(m.input)
    expect(m.input.selectionStart).toBe(5)
    expect(m.outerKeys).toEqual([])
  })

  describe('opening on its own', () => {
    /** Past the caret's and the pointer's open and close delays. */
    const pause = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 450))

    async function caretAt(m: Mounted, caret: number): Promise<void> {
      m.input.focus()
      m.input.setSelectionRange(caret, caret)
      m.input.dispatchEvent(new Event('keyup', { bubbles: true }))
      await pause()
      await settle()
    }

    it('opens when the caret rests inside a counter, and leaves focus in the field', async () => {
      const m = await mountField('IMG_[C10] x')
      await caretAt(m, 6)
      expect(editorFields().map((f) => f.value)).toEqual(['10', '1', '1'])
      expect(document.activeElement).toBe(m.input)
    })

    it('stays shut with the caret right after a token, so finishing one never pops it', async () => {
      const m = await mountField('IMG_[C10] x')
      await caretAt(m, 9)
      expect(document.querySelector('.ui-popover')).toBeNull()
    })

    it('closes once the caret leaves the token', async () => {
      const m = await mountField('IMG_[C10] x')
      await caretAt(m, 6)
      await caretAt(m, 1)
      expect(document.querySelector('.ui-popover')).toBeNull()
    })

    it('ArrowDown moves from the field into the open editor’s first field', async () => {
      const m = await mountField('IMG_[C10] x')
      await caretAt(m, 6)
      key(m.input, 'ArrowDown')
      await settle()
      expect(document.activeElement).toBe(editorFields()[0])
      expect(m.outerKeys).toEqual([])
    })

    it('Escape closes it from the field without reaching the sheet', async () => {
      const m = await mountField('IMG_[C10] x')
      await caretAt(m, 6)
      key(m.input, 'Escape')
      await settle()
      expect(document.querySelector('.ui-popover')).toBeNull()
      expect(m.outerKeys).toEqual([])
      expect(document.activeElement).toBe(m.input)
    })

    it('closes when the token stops being a counter', async () => {
      const m = await mountField('IMG_[C10] x')
      await caretAt(m, 6)
      type(m.input, 'IMG_[C1x0] x')
      m.input.setSelectionRange(7, 7)
      await settle()
      expect(document.querySelector('.ui-popover')).toBeNull()
    })

    it('opens on hover after a moment, and closes a moment after the pointer leaves', async () => {
      await mountField('[C] x [C+5]')
      const marker = document.querySelectorAll<HTMLButtonElement>('button[aria-label="Edit counter"]')[1]
      marker.dispatchEvent(new MouseEvent('mouseenter'))
      await pause()
      await settle()
      expect(editorFields().map((f) => f.value)).toEqual(['1', '5', '1'])
      marker.dispatchEvent(new MouseEvent('mouseleave'))
      marker.parentElement?.dispatchEvent(new MouseEvent('mouseleave'))
      await pause()
      await settle()
      expect(document.querySelector('.ui-popover')).toBeNull()
    })

    it('stays open while the pointer moves from the token into the editor', async () => {
      await mountField('[C] x')
      const marker = document.querySelector<HTMLButtonElement>('button[aria-label="Edit counter"]')
      marker?.dispatchEvent(new MouseEvent('mouseenter'))
      await pause()
      await settle()
      marker?.parentElement?.dispatchEvent(new MouseEvent('mouseleave'))
      document.querySelector('.ui-popover')?.dispatchEvent(new MouseEvent('mouseenter'))
      await pause()
      await settle()
      expect(document.querySelector('.ui-popover')).not.toBeNull()
    })
  })
})
