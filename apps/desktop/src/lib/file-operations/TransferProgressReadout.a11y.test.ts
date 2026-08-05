import { describe, it, beforeEach, vi } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import TransferProgressReadout from './TransferProgressReadout.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { seconds } from '$lib/units'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

const running = {
  bytesDone: 50,
  bytesTotal: 200,
  filesDone: 1,
  filesTotal: 4,
  bytesPerSecond: 1_500_000,
  filesPerSecond: 27,
  etaSeconds: seconds(154),
}

beforeEach(() => {
  document.body.innerHTML = ''
})

async function mountReadout(props: ComponentProps<typeof TransferProgressReadout>): Promise<HTMLElement> {
  const host = document.createElement('div')
  document.body.appendChild(host)
  mount(TransferProgressReadout, { target: host, props })
  await tick()
  return host
}

describe('TransferProgressReadout a11y', () => {
  it('the dialog density has no a11y violations', async () => {
    await expectNoA11yViolations(await mountReadout(running))
  })

  it('the compact list-row density has no a11y violations', async () => {
    await expectNoA11yViolations(await mountReadout({ ...running, density: 'compact' }))
  })

  it('a stalled readout with no size total has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountReadout({
        ...running,
        bytesTotal: 0,
        countKind: 'items',
        stall: { stillForSeconds: 45, reason: 'destination', inFlight: 2 },
      }),
    )
  })
})
