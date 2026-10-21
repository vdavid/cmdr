/**
 * Behavior tests for the range editor inside `MaskInput`: a range token gets a marker, ↓ opens the
 * editor on its start handle, a drag or a key rewrites the token (each end counting from its nearer
 * edge on a drag, keeping the token's own edges on a key), the Start / End / Write as rows rewrite
 * how it counts, and the files around the cursor show what it takes. The engine is a fake that
 * answers fixed bounds per token: the real bounds are `mask.rs`'s, tested there.
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
  'N1--2': [0, 7],
  'N1-3': [0, 2],
  'N2-4': [1, 3],
  'N1-5': [0, 4],
  'N1--4': [0, 5],
  'N-9-5': [0, 4],
  'N1,5': [0, 4],
  'N20-25': null,
  'E1-5': [0, 2],
}

function fakeSource(): RangeSampleSource & { asked: string[] } {
  const asked: string[] = []
  return {
    asked,
    sampleRows: { cursor: 0, rows: [0, 1, 2] },
    rangeSamples: (token) => {
      asked.push(token)
      const taken = TAKEN[token] ?? null
      const samples: RangeSample[] = [
        {
          row: 0,
          text: token.startsWith('E') ? 'jpg' : 'Beach day',
          taken: taken ? { first: taken[0], last: taken[1] } : null,
        },
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

function key(el: Element, k: string, mods: { shiftKey?: boolean; altKey?: boolean } = {}): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...mods }))
}

async function open(m: Mounted, caret: number): Promise<void> {
  m.input.focus()
  m.input.setSelectionRange(caret, caret)
  key(m.input, 'ArrowDown')
  await settle()
}

const handle = (name: 'Start' | 'End'): HTMLElement | null =>
  document.querySelector<HTMLElement>(`.ui-popover [role="slider"][aria-label="${name}"]`)

function cell(index: number): HTMLElement {
  const found = document.querySelector<HTMLElement>(`.ui-popover [data-index="${String(index)}"]`)
  if (!found) throw new Error(`no cell ${String(index)}`)
  return found
}

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

  it('leaves ↓ right after a range to the field’s history', async () => {
    const m = await mountField('[N]')
    await open(m, 3)
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(m.outerKeys).toContain('ArrowDown')
  })

  it('opens on ↓ with the start handle focused, over the cursor file’s characters', async () => {
    const m = await mountField('[N]')
    await open(m, 2)
    expect(document.querySelectorAll('.ui-popover .cell')).toHaveLength(9)
    expect(document.activeElement).toBe(handle('Start'))
    expect(handle('Start')?.getAttribute('aria-valuetext')).toBe('character 1, B')
    // `[N]` runs to the end, so its end counts from the end.
    expect(handle('End')?.getAttribute('aria-valuetext')).toBe('the last character, y')
    expect(document.querySelector('.ui-popover .reading')?.textContent).toContain('All of it')
  })

  it('moves the end with ⇧←, the whole selection with →, and the start with ⌥→, keeping the token’s edges', async () => {
    const m = await mountField('[N]')
    await open(m, 2)
    key(handle('Start') as HTMLElement, 'ArrowLeft', { shiftKey: true })
    await settle()
    expect(m.value()).toBe('[N1--2]')
    expect(document.activeElement).toBe(handle('End'))

    const n = await remount('[N1-3]')
    key(handle('Start') as HTMLElement, 'ArrowRight')
    await settle()
    expect(n.value()).toBe('[N2-4]')
  })

  it('counts each end of a drag from its nearer edge', async () => {
    const m = await mountField('[N]')
    await open(m, 2)
    cell(0).dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }))
    cell(4).dispatchEvent(new PointerEvent('pointermove', { bubbles: true, button: 0 }))
    await settle()
    expect(m.value()).toBe('[N1-5]')
    cell(5).dispatchEvent(new PointerEvent('pointermove', { bubbles: true, button: 0 }))
    cell(5).dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0 }))
    await settle()
    expect(m.value()).toBe('[N1--4]')
  })

  it('counts the start from the end on request, which takes the end with it', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    expect(toggle('5th').disabled).toBe(false)
    toggle('9th from the end').click()
    await settle()
    expect(m.value()).toBe('[N-9-5]')
    expect(toggle('5th').disabled).toBe(true)
  })

  it('writes a start and length, and drops the End row', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    toggle('Start and length').click()
    await settle()
    expect(m.value()).toBe('[N1,5]')
    expect(handle('End')).not.toBeNull()
    expect([...document.querySelectorAll('.ui-popover .label')].map((label) => label.textContent)).not.toContain('End')
  })

  it('switches the field and keeps the range', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    toggle('Extension').click()
    await settle()
    expect(m.value()).toBe('[E1-5]')
  })

  it('shows what the token takes from the files around the cursor, the cursor’s set apart', async () => {
    const m = await mountField('[N1-5]')
    await open(m, 2)
    const rows = [...document.querySelectorAll<HTMLElement>('.ui-popover .sample')]
    expect(rows.map((row) => row.querySelector('.result')?.textContent)).toEqual(['Beach', 'IMG_', 'nothing'])
    expect(rows[0].getAttribute('aria-current')).toBe('true')
    expect(rows[0].querySelector('mark')?.textContent).toBe('Beach')
  })

  it('says when it takes nothing from the cursor’s file, and a key then picks the first character', async () => {
    const m = await mountField('[N20-25]')
    await open(m, 7)
    expect(document.querySelector('.ui-popover .warning')?.textContent).toBe('Takes nothing from this file')
    expect(handle('Start')).toBeNull()
    const ribbon = document.querySelector<HTMLElement>('.ui-popover .ribbon')
    expect(document.activeElement).toBe(ribbon)
    key(ribbon as HTMLElement, 'ArrowRight')
    await settle()
    expect(m.value()).toBe('[N1]')
  })

  it('walks the rows with ↑ / ↓ and leaves from the top, and Enter leaves from anywhere', async () => {
    const m = await mountField('[N] x')
    await open(m, 2)
    key(handle('Start') as HTMLElement, 'ArrowDown')
    await settle()
    expect(document.activeElement?.textContent.trim()).toBe('1st')
    key(document.activeElement as HTMLElement, 'ArrowUp')
    key(document.activeElement as HTMLElement, 'ArrowUp')
    await settle()
    expect(document.activeElement?.textContent.trim()).toBe('Name')
    key(document.activeElement as HTMLElement, 'ArrowUp')
    await settle()
    expect(document.activeElement).toBe(m.input)
    expect(m.input.selectionStart).toBe(3)

    await open(m, 2)
    key(handle('Start') as HTMLElement, 'Enter')
    await settle()
    expect(document.activeElement).toBe(m.input)
    expect(m.outerKeys).not.toContain('Enter')
    expect(m.outerKeys).not.toContain('ArrowUp')
  })

  it('follows the mask as the user types in it', async () => {
    const m = await mountField('[N]')
    await open(m, 2)
    m.input.value = '[N1-3]'
    m.input.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    expect(document.querySelector('.ui-popover .reading')?.textContent).toContain('From the 1st to the 3rd character')
  })
})

/** Mounts a fresh field (the last one goes) and opens its editor on the token at the start. */
async function remount(initial: string): Promise<Mounted> {
  cleanup?.()
  document.body.innerHTML = ''
  const m = await mountField(initial)
  await open(m, 2)
  return m
}
