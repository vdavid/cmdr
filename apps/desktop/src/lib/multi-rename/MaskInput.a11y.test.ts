/**
 * Tier 3 a11y tests for `MaskInput.svelte`: the field with its counter markers, and the counter
 * editor open under a token.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MaskInputFixture from './mask-input-fixture.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

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
  it('the field with its counter markers has no violations', async () => {
    const root = await mountField()
    if (root.querySelectorAll('button').length !== 2) throw new Error('the markers did not render')
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
