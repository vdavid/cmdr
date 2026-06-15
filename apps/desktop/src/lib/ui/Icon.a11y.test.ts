/**
 * Tier 3 a11y tests for `Icon.svelte`.
 *
 * Icon renders an inline glyph from the shared registry. It carries no ARIA of its own: a
 * decorative glyph takes `aria-hidden`, a meaningful one takes `role="img"` + `aria-label`, both
 * passed through by the caller. axe confirms each shape produces clean markup. Contrast is a
 * wrapper concern (the glyph inherits `currentColor`), so there's none to validate here.
 */
import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import Icon from './Icon.svelte'
import { ICON_COMPONENTS, type IconName } from './icons/icon-map'
import { expectNoA11yViolations } from '$lib/test-a11y'

async function renderIcon(props: ComponentProps<typeof Icon>): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(Icon, { target, props })
  await tick()
  return target
}

describe('Icon a11y', () => {
  it('decorative icon (aria-hidden) has no a11y violations', async () => {
    const target = await renderIcon({ name: 'triangle-alert', size: 16, 'aria-hidden': 'true' })
    await expectNoA11yViolations(target)
  })

  it('meaningful icon (role=img + aria-label) has no a11y violations', async () => {
    const target = await renderIcon({
      name: 'hourglass',
      size: 12,
      role: 'img',
      'aria-label': 'Size updating',
    })
    await expectNoA11yViolations(target)
  })

  it('custom glyph (eject) renders and has no a11y violations', async () => {
    const target = await renderIcon({ name: 'eject', size: 14, 'aria-hidden': 'true' })
    await expectNoA11yViolations(target)
  })

  it('every registered glyph renders without throwing', async () => {
    for (const name of Object.keys(ICON_COMPONENTS) as IconName[]) {
      const target = await renderIcon({ name, size: 16, 'aria-hidden': 'true' })
      await expectNoA11yViolations(target)
    }
  })
})
