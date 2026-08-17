/**
 * The mouse and keyboard contract of `InlineRenameEditor.svelte`.
 *
 * The mouse half is the load-bearing part: the editor must stay usable with a
 * mouse (place the caret, drag-select), and a press outside it means "save",
 * while losing focus for structural reasons (the row scrolling out of the
 * virtual window, which unmounts the input) still means "discard".
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, unmount, tick, type ComponentProps } from 'svelte'
import InlineRenameEditor from './InlineRenameEditor.svelte'

const noop = () => {}

function setup(overrides: Record<string, unknown> = {}) {
  const handlers = {
    onInput: vi.fn(),
    onSubmit: vi.fn(),
    onCancel: vi.fn(),
    onClickAway: vi.fn(),
    onShakeEnd: vi.fn(),
  }
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(InlineRenameEditor, {
    target,
    props: {
      value: 'report.md',
      severity: 'ok',
      shaking: false,
      ariaLabel: 'Rename report.md',
      sessionId: 1,
      ...handlers,
      ...overrides,
    },
  })
  const input = target.querySelector('.rename-input') as HTMLInputElement
  return { target, component, input, ...handlers }
}

/** A real, bubbling press, the way a click into the pane arrives. */
function pressOn(el: Element) {
  el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('InlineRenameEditor mouse handling', () => {
  it('a press INSIDE the input is left alone, so the caret can be placed', async () => {
    const { input, onClickAway, onCancel } = setup()
    await tick()

    pressOn(input)

    expect(onClickAway).not.toHaveBeenCalled()
    expect(onCancel).not.toHaveBeenCalled()
  })

  it('a press OUTSIDE the input commits', async () => {
    const { onClickAway, onCancel } = setup()
    await tick()

    pressOn(document.body)

    expect(onClickAway).toHaveBeenCalledTimes(1)
    expect(onCancel).not.toHaveBeenCalled()
  })

  it('sees the press before anything else can move focus (capture phase)', async () => {
    const { onClickAway } = setup()
    await tick()
    const row = document.createElement('div')
    document.body.appendChild(row)
    // A bubble-phase listener that stops propagation, like a row handler would.
    row.addEventListener('mousedown', (e) => {
      e.stopPropagation()
    })

    pressOn(row)

    expect(onClickAway).toHaveBeenCalledTimes(1)
  })

  it('stops listening once unmounted, so a later click cannot resurrect the rename', async () => {
    const { component, onClickAway } = setup()
    await tick()

    void unmount(component)
    await tick()
    pressOn(document.body)

    expect(onClickAway).not.toHaveBeenCalled()
  })
})

describe('InlineRenameEditor keyboard and focus', () => {
  it('Enter submits, Escape and Tab discard', async () => {
    const { input, onSubmit, onCancel } = setup()
    await tick()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(onSubmit).toHaveBeenCalledTimes(1)

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(onCancel).toHaveBeenCalledTimes(1)

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(onCancel).toHaveBeenCalledTimes(2)
  })

  it('losing focus with no click behind it still discards (the row scrolled away)', async () => {
    const { input, onCancel, onClickAway } = setup()
    await tick()

    input.dispatchEvent(new FocusEvent('blur'))

    expect(onCancel).toHaveBeenCalledTimes(1)
    expect(onClickAway).not.toHaveBeenCalled()
  })

  it('names the rename session it belongs to when it discards', async () => {
    const { input, onCancel } = setup({ sessionId: 7 })
    await tick()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    input.dispatchEvent(new FocusEvent('blur'))

    expect(onCancel).toHaveBeenNthCalledWith(1, 7)
    expect(onCancel).toHaveBeenNthCalledWith(2, 7)
  })

  it('names the session it was OPENED for, so its parting blur cannot end a newer one', async () => {
    // Renaming down a run of files replaces this editor with one on the next
    // file. The outgoing input blurs as it unmounts; reporting the session that
    // is live BY THEN would discard the edit the user has already started.
    // The live-reading getter is what a reactive `sessionId` read would see.
    let liveSession = 4
    const onCancel = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = {
      value: 'report.md',
      severity: 'ok',
      shaking: false,
      ariaLabel: 'Rename report.md',
      onInput: noop,
      onSubmit: noop,
      onCancel,
      onClickAway: noop,
      onShakeEnd: noop,
      get sessionId() {
        return liveSession
      },
    }
    mount(InlineRenameEditor, { target, props: props as ComponentProps<typeof InlineRenameEditor> })
    const input = target.querySelector('.rename-input') as HTMLInputElement
    await tick()

    liveSession = 5
    input.dispatchEvent(new FocusEvent('blur'))

    expect(onCancel).toHaveBeenCalledWith(4)
  })

  it('takes focus and selects the name without its extension on mount', async () => {
    const { input } = setup({ value: 'report.md', onInput: noop })
    await tick()
    await tick()

    expect(document.activeElement).toBe(input)
    expect(input.selectionStart).toBe(0)
    expect(input.selectionEnd).toBe('report'.length)
  })
})
