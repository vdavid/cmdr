/**
 * Tier 3 a11y tests for `MaskInput.svelte`: the field with its counter and range markers, the counter
 * editor open under a token, and the range editor open with files to show.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

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

async function mountField(): Promise<HTMLElement> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const fixture = mount(MaskInputFixture, {
    target: root,
    props: { initial: 'IMG_[C10+2:3] [N] [C]', onValue: () => {}, onOuterKey: () => {} },
  })
  cleanup = () => {
    void unmount(fixture)
  }
  await settle()
  return root
}

describe('MaskInput a11y', () => {
  it('the field with its counter and range markers has no violations', async () => {
    const root = await mountField()
    if (root.querySelectorAll('button').length !== 3) throw new Error('the markers did not render')
    await expectNoA11yViolations(root)
  })

  it('the open counter editor has no violations', async () => {
    const root = await mountField()
    root.querySelector<HTMLButtonElement>('button')?.click()
    await settle()
    if (!document.querySelector('.ui-popover')) throw new Error('the editor did not open')
    await expectNoA11yViolations(document.body)
  })
})

describe('the range editor a11y', () => {
  async function mountRange(initial: string): Promise<HTMLElement> {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const source = {
      sampleRows: { cursor: 0, rows: [0, 1] },
      rangeSamples: (token: string) =>
        Promise.resolve([
          { row: 0, text: 'Beach day', taken: token === 'N20' ? null : { first: 0, last: 4 } },
          { row: 1, text: 'x', taken: null },
        ]),
    }
    const fixture = mount(MaskInputFixture, {
      target: root,
      props: { initial, onValue: () => {}, onOuterKey: () => {}, source },
    })
    cleanup = () => {
      void unmount(fixture)
    }
    await settle()
    const input = root.querySelector('input')
    input?.focus()
    input?.setSelectionRange(2, 2)
    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }))
    await settle()
    if (!document.querySelector('.ui-popover [role="slider"], .ui-popover .warning')) {
      throw new Error('the range editor did not open')
    }
    return root
  }

  it('the open range editor, its handles as sliders, has no violations', async () => {
    await mountRange('[N1--5]')
    await expectNoA11yViolations(document.body)
  })

  it('the range editor taking nothing has no violations', async () => {
    await mountRange('[N20]')
    await expectNoA11yViolations(document.body)
  })

  it('the range editor with its start counted from the end, End’s other option off, has no violations', async () => {
    await mountRange('[N-9-5]')
    await expectNoA11yViolations(document.body)
  })
})
