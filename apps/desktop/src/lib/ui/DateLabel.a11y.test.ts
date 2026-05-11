import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import DateLabel from './DateLabel.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formattedDate: (t: number | undefined) =>
    t
      ? {
          text: '2025-03-14 10:30',
          parts: {
            left: [
              { text: '2025', ageClass: 'age-fresh' as const },
              { text: '-03-14', ageClass: null },
            ],
            right: [
              { text: '10:30', ageClass: null },
            ],
          },
        }
      : { text: '', parts: { left: [], right: null } },
}))

describe('DateLabel a11y', () => {
  it('with a timestamp has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DateLabel, { target, props: { modifiedAt: 1710409800 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a null timestamp has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DateLabel, { target, props: { modifiedAt: null } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
