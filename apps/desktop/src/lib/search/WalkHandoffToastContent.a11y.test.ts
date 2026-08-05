import { describe, it, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

import WalkHandoffToastContent from './WalkHandoffToastContent.svelte'
import { _resetWalkHandoffForTesting } from './walk-handoff.svelte'
import { setWalkHandoff } from './walk-handoff-state.svelte'

afterEach(() => {
  _resetWalkHandoffForTesting()
})

describe('WalkHandoffToastContent a11y', () => {
  it('renders with no a11y violations while a walk is running', async () => {
    // The component is prop-free by design (a toast replaced in place keeps its
    // original props), so the module state IS the fixture here.
    setWalkHandoff({
      runId: 'run-1',
      snapshotId: 'sr-1',
      label: '*.pdf',
      view: {
        phase: 'walking',
        matchCount: 1234,
        dirsFound: 5678,
        currentPath: '/Volumes/Backups/photos/2019',
        capped: false,
        running: true,
        incomplete: false,
      },
    })
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(WalkHandoffToastContent, { target })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders nothing, and nothing broken, with no walk to speak about', async () => {
    setWalkHandoff(null)
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(WalkHandoffToastContent, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
