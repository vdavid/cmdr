/**
 * Tier 3 a11y tests for `DirectionIndicator.svelte`.
 *
 * Arrow graphic that shows "source folder -> destination folder" or the
 * reverse. No Tauri deps, just pure presentational component.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import DirectionIndicator from './DirectionIndicator.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('DirectionIndicator a11y', () => {
  it('right direction (source -> destination) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DirectionIndicator, {
      target,
      props: {
        sourcePath: '/Users/test/documents',
        destinationPath: '/Users/test/backup',
        direction: 'right',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('left direction (destination <- source) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DirectionIndicator, {
      target,
      props: {
        sourcePath: '/Users/test/source-folder',
        destinationPath: '/Users/test/target-folder',
        direction: 'left',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long paths (truncated) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DirectionIndicator, {
      target,
      props: {
        sourcePath: '/Users/test/nested/deeply/inside/a-very-long-folder-name-that-overflows',
        destinationPath: '/Volumes/External/backup/2026/january/archive/another-very-long-folder-name',
        direction: 'right',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
