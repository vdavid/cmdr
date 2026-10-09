/**
 * Tier 3 a11y tests for `CounterTokenEditor.svelte`: the three labelled number fields and the
 * line saying what they count, plain and with padding.
 */

import { afterEach, describe, it } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import CounterTokenEditor from './CounterTokenEditor.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  document.body.innerHTML = ''
})

async function mountEditor(value: { start: number; step: number; digits: number }): Promise<HTMLElement> {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const editor = mount(CounterTokenEditor, { target: root, props: { value, onChange: () => {}, onDone: () => {} } })
  cleanup = () => {
    void unmount(editor)
  }
  await tick()
  return root
}

describe('CounterTokenEditor a11y', () => {
  it('the default counter has no violations', async () => {
    await expectNoA11yViolations(await mountEditor({ start: 1, step: 1, digits: 1 }))
  })

  it('a padded counter, counting down, has no violations', async () => {
    await expectNoA11yViolations(await mountEditor({ start: 10, step: -2, digits: 3 }))
  })
})
