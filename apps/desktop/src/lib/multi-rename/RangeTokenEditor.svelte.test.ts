/**
 * Behavior tests for the range editor inside `MaskInput`: a range token gets a marker, ↓ opens the
 * editor in its text box with the token's characters selected, a selection the user makes rewrites
 * the token (each end counting from its nearer edge on a pointer pick, keeping the token's own
 * edges on a key), a token changed any other way moves the box's selection, the Start / End /
 * Write as rows rewrite how it counts, and the files around the cursor show what it takes. The
 * engine is a fake that answers fixed bounds per token: the real bounds are `mask.rs`'s, tested there.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'
import type { RangeSampleSource } from './range-sample-source'
import type { RangeSample } from '#lib/tauri-commands/index.js'

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

/** What the fake engine takes from "Beach day" (or "jpg" for `[E…]`) for each token the tests write. */
const TAKEN: Record<string, [number, number] | null> = {
  N: [0, 8],
  'N1-3': [0, 2],
  'N1-4': [0, 3],
  'N1-5': [0, 4],
  'N1--4': [0, 5],
  'N-9-5': [0, 4],
  'N1,5': [0, 4],
  N1: [0, 0],
  'N20-25': null,
}

/** The field's text on the cursor's file: `[G…]` is empty there. */
function textFor(token: string): string {
  if (token.startsWith('E')) return 'jpg'
  if (token.startsWith('G')) return ''
  return 'Beach day'
}

function fakeSource(): RangeSampleSource {
  return {
    sampleRows: { cursor: 0, rows: [0, 1, 2] },
    rangeSamples: (token) => {
      const taken = TAKEN[token] ?? null
      const samples: RangeSample[] = [
        { row: 0, text: textFor(token), taken: taken ? { first: taken[0], last: taken[1] } : null },
        { row: 1, text: 'IMG_2041', taken: { first: 0, last: 3 } },
        { row: 2, text: 'x', taken: null },
      ]
      return Promise.resolve(samples)
    },
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
      source: fakeSource(),
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

function key(el: Element, k: string, mods: { shiftKey?: boolean } = {}): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...mods }))
}

async function open(m: Mounted, caret: number): Promise<void> {
  m.input.focus()
  m.input.setSelectionRange(caret, caret)
  key(m.input, 'ArrowDown')
  await settle()
}

function box(): HTMLInputElement {
  const found = document.querySelector<HTMLInputElement>('.ui-popover input[readonly]')
  if (!found) throw new Error('no text box')
  return found
}

/** The user selecting `start`..`end` in the box, by pointer or by key. */
async function select(by: 'pointer' | 'key', start: number, end: number): Promise<void> {
  const el = box()
  if (by === 'pointer') el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }))
  else key(el, 'ArrowRight', { shiftKey: true })
  el.setSelectionRange(start, end)
  el.dispatchEvent(new Event('select', { bubbles: true }))
  await settle()
}

const shown = (): [number | null, number | null] => [box().selectionStart, box().selectionEnd]

function toggle(label: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('.ui-popover button')].find(
    (button) => button.textContent.trim() === label,
  )
  if (!found) throw new Error(`no ${label} option`)
  return found
}

describe('the range editor', () => {
  it('marks every range token, the full-name form too', async () => {
    await mountField('[N] [E2-] [C] [2-5]')
    expect(document.querySelectorAll('button[aria-label="Choose characters"]')).toHaveLength(3)
  })

  describe('opens only with the caret inside the token, never on its outer edges', () => {
    /** Past the caret's and the pointer's open delays. */
    const pause = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 450))

    async function caretRestsAt(m: Mounted, caret: number): Promise<void> {
      m.input.focus()
      m.input.setSelectionRange(caret, caret)
      m.input.dispatchEvent(new Event('keyup', { bubbles: true }))
      await pause()
      await settle()
    }

    // `x [N2-5] y`: `[` at 2, `]` at 7, so 2 is `|[`, 3 is `[|`, 7 is `|]`, and 8 is `]|`.
    const MASK = 'x [N2-5] y'

    it.each([2, 8])('leaves ↓ at %i, on an outer edge, to the field’s history', async (caret) => {
      const m = await mountField(MASK)
      await open(m, caret)
      expect(document.querySelector('.ui-popover')).toBeNull()
      expect(m.outerKeys).toContain('ArrowDown')
    })

    it.each([3, 5, 7])('opens on ↓ at %i, inside the token', async (caret) => {
      const m = await mountField(MASK)
      await open(m, caret)
      expect(document.activeElement).toBe(box())
      expect(m.outerKeys).not.toContain('ArrowDown')
    })

    it.each([2, 8])('stays shut with the caret resting at %i, on an outer edge', async (caret) => {
      const m = await mountField(MASK)
      await caretRestsAt(m, caret)
      expect(document.querySelector('.ui-popover')).toBeNull()
    })

    it.each([3, 5, 7])('opens passively with the caret resting at %i, inside the token', async (caret) => {
      const m = await mountField(MASK)
      await caretRestsAt(m, caret)
      expect(document.querySelector('.ui-popover')).not.toBeNull()
      expect(document.activeElement).toBe(m.input)
    })

    it.each([2, 8])('doesn’t take ↓ at %i into an editor the pointer opened', async (caret) => {
      const m = await mountField(MASK)
      m.input.focus()
      m.input.setSelectionRange(caret, caret)
      document.querySelector('button[aria-label="Choose characters"]')?.dispatchEvent(new MouseEvent('mouseenter'))
      await pause()
      await settle()
      expect(document.querySelector('.ui-popover')).not.toBeNull()
      key(m.input, 'ArrowDown')
      await settle()
      expect(document.activeElement).toBe(m.input)
      expect(m.outerKeys).toContain('ArrowDown')
    })
  })

  it('opens on ↓ in its labelled text box, the cursor file’s name in it and the token’s characters selected', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    expect(document.activeElement).toBe(box())
    expect(box().value).toBe('Beach day')
    expect(shown()).toEqual([0, 5])
    expect(box().labels?.[0]?.textContent).toBe('Select the characters to take')
    expect(document.querySelector('.ui-popover .reading')?.textContent).toContain('From the 1st to the 5th character')
  })

  it('writes a pointer selection as the token, each end counting from its nearer edge', async () => {
    const m = await mountField('[N]')
    await open(m, 2)
    await select('pointer', 0, 5)
    expect(m.value()).toBe('[N1-5]')
    await select('pointer', 0, 6)
    expect(m.value()).toBe('[N1--4]')
  })

  it('keeps the token’s own edges when a key adjusts the selection', async () => {
    const m = await mountField('[N1-3]')
    await open(m, 2)
    // A pointer pick would count an end on the 6th of nine from the end (`[N1--4]`); a key keeps it from the start.
    await select('key', 0, 4)
    expect(m.value()).toBe('[N1-4]')
  })

  it('leaves the token as it is for a bare caret', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    await select('pointer', 3, 3)
    expect(m.value()).toBe('[N1-5]')
  })

  it('ignores a selection the user didn’t make, like Tab’s select-all, and puts the token’s characters back', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    box().blur()
    box().focus()
    box().setSelectionRange(0, 9)
    box().dispatchEvent(new Event('select', { bubbles: true }))
    await settle()
    expect(m.value()).toBe('[N1-5]')
    expect(shown()).toEqual([0, 5])
  })

  it('moves the box’s selection when the token changes in the mask', async () => {
    const m = await mountField('[N]')
    await open(m, 2)
    expect(shown()).toEqual([0, 9])
    m.input.value = '[N1-3]'
    m.input.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    expect(document.querySelector('.ui-popover .reading')?.textContent).toContain('From the 1st to the 3rd character')
    expect(shown()).toEqual([0, 3])
  })

  it('counts the start from the end on request, which takes the end with it', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    expect(toggle('5th').disabled).toBe(false)
    toggle('9th from the end').click()
    await settle()
    expect(m.value()).toBe('[N-9-5]')
    expect(toggle('5th').disabled).toBe(true)
    expect(shown()).toEqual([0, 5])
  })

  it('writes a start and length, and drops the End row', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    toggle('Start and length').click()
    await settle()
    expect(m.value()).toBe('[N1,5]')
    expect([...document.querySelectorAll('.ui-popover .label')].map((label) => label.textContent)).not.toContain('End')
  })

  it('shows what the token takes from the files around the cursor, the cursor’s set apart', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    const rows = [...document.querySelectorAll<HTMLElement>('.ui-popover .sample')]
    expect(rows.map((row) => row.querySelector('.result')?.textContent)).toEqual(['Beach', 'IMG_', 'nothing'])
    expect(rows[0].getAttribute('aria-current')).toBe('true')
    expect(rows[0].querySelector('mark')?.textContent).toBe('Beach')
  })

  it('says when it takes nothing from the cursor’s file, selects nothing, and a selection then picks', async () => {
    const m = await mountField('[N20-25]')
    await open(m, 7)
    expect(document.querySelector('.ui-popover .warning')?.textContent).toBe('Takes nothing from this file')
    expect(shown()).toEqual([0, 0])
    await select('pointer', 0, 1)
    expect(m.value()).toBe('[N1]')
  })

  it('says so when the field is empty on the cursor’s file', async () => {
    const m = await mountField('[G]')
    await open(m, 2)
    expect(box().value).toBe('')
    expect(box().placeholder).toBe('Empty for this file')
  })

  it('walks the rows with ↑ / ↓ and leaves from the box, and Enter leaves from anywhere', async () => {
    const m = await mountField('[N] x')
    await open(m, 2)
    key(box(), 'ArrowDown')
    await settle()
    expect(document.activeElement?.textContent.trim()).toBe('1st')
    key(document.activeElement as HTMLElement, 'ArrowUp')
    await settle()
    expect(document.activeElement).toBe(box())
    key(box(), 'ArrowUp')
    await settle()
    expect(document.activeElement).toBe(m.input)
    expect(m.input.selectionStart).toBe(3)

    await open(m, 2)
    key(box(), 'Enter')
    await settle()
    expect(document.activeElement).toBe(m.input)
    expect(m.outerKeys).not.toContain('Enter')
    expect(m.outerKeys).not.toContain('ArrowUp')
    expect(m.value()).toBe('[N] x')
  })
})
