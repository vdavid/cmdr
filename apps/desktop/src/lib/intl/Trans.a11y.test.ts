import { describe, it, beforeAll, afterAll } from 'vitest'
import { mount, tick } from 'svelte'
import TransFixture from './trans-fixture.svelte'
import { _setLocaleForTests } from './locale'
import { expectNoA11yViolations } from '$lib/test-a11y'

// `<Trans>` renders a sentence with an inline interactive component (the FDA
// hint's `<settingsLink>` → a `LinkButton`). It carries no a11y surface of its
// own, but the rendered text + inline control must be axe-clean. We mount
// through `trans-fixture.svelte` (the real call shape: a `<Trans>` whose tag
// maps to a `LinkButton` snippet).

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('Trans a11y', () => {
  it('an inline-component sentence has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TransFixture, { target, props: { messageKey: 'common.downloadsFdaHint' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
