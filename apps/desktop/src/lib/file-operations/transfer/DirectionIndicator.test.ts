/**
 * Component tests for `DirectionIndicator.svelte`'s label derivation.
 *
 * The direction header shows "source -> destination". Normally each side's
 * label is the path basename, but at a volume root the basename can be a raw
 * machine id (an MTP storage id like "65538"). The optional `sourceLabel` /
 * `destinationLabel` overrides let the caller substitute a volume display name.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import DirectionIndicator from './DirectionIndicator.svelte'

function mountIndicator(props: {
  sourcePath: string
  destinationPath: string
  direction: 'left' | 'right'
  sourceLabel?: string
  destinationLabel?: string
}): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(DirectionIndicator, { target, props })
  return target
}

function sourceText(target: HTMLElement): string {
  return target.querySelector('.folder-name.source')?.textContent.trim() ?? ''
}

function destinationText(target: HTMLElement): string {
  return target.querySelector('.folder-name.destination')?.textContent.trim() ?? ''
}

describe('DirectionIndicator label derivation', () => {
  it('renders the path basename when no label override is given', async () => {
    const target = mountIndicator({
      sourcePath: '/Users/test/photos',
      destinationPath: '/Users/test/backup',
      direction: 'right',
    })
    await tick()
    expect(sourceText(target)).toBe('photos')
    expect(destinationText(target)).toBe('backup')
  })

  it('renders the source label override instead of an MTP storage-id basename', async () => {
    const target = mountIndicator({
      // At an MTP storage root, the basename is the raw storage id "65538".
      sourcePath: '/mtp-20-5/65538',
      destinationPath: '/Users/test/backup',
      direction: 'left',
      sourceLabel: 'Virtual Pixel 9 - SD Card',
    })
    await tick()
    expect(sourceText(target)).toBe('Virtual Pixel 9 - SD Card')
    expect(sourceText(target)).not.toContain('65538')
  })

  it('renders the destination label override when provided', async () => {
    const target = mountIndicator({
      sourcePath: '/Users/test/photos',
      destinationPath: '/mtp-20-5/65538',
      direction: 'right',
      destinationLabel: 'Virtual Pixel 9 - SD Card',
    })
    await tick()
    expect(destinationText(target)).toBe('Virtual Pixel 9 - SD Card')
  })
})
