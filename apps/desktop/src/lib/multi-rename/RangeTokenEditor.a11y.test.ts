/**
 * Tier 3 a11y tests for `RangeTokenEditor.svelte`: on its own, with no files to show (a field outside
 * the sheet), and open under a range token in `MaskInput` with files around the cursor, its handles
 * as sliders.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'
import RangeTokenEditor from './RangeTokenEditor.svelte'
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

describe('RangeTokenEditor a11y', () => {
  it('the editor with no files to show has no violations', async () => {
    const root = document.createElement('div')
    document.body.appendChild(root)
    const editor = mount(RangeTokenEditor, {
      target: root,
      props: { value: { field: 'name', range: { kind: 'span', at: 2, to: 5 } }, onChange: () => {}, onDone: () => {} },
    })
    cleanup = () => {
      void unmount(editor)
    }
    await settle()
    await expectNoA11yViolations(root)
  })
})

describe('the range editor open in a mask field a11y', () => {
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
