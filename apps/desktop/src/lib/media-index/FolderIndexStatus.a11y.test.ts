/**
 * Tier 3 a11y + behavior tests for `FolderIndexStatus.svelte` (the pane status bar's
 * image-search readout).
 *
 * The load-bearing assertions are the honest ones: nothing renders while image search is
 * off or on a pane whose path isn't an OS path, and a folder the settings don't cover
 * never reads as indexed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import FolderIndexStatus from './FolderIndexStatus.svelte'

const settings: Record<string, unknown> = {
  'mediaIndex.enabled': true,
  'mediaIndex.scope': 'chosen',
  'mediaIndex.alwaysIndexFolders': [],
  'mediaIndex.excludedFolders': [],
}

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => settings[id],
  onSpecificSettingChange: () => () => {},
}))

const enrichActivity = vi.fn<(volumeId: string) => unknown>(() => undefined)

vi.mock('$lib/indexing/media-enrich-state.svelte', () => ({
  getVolumeEnrichActivity: (volumeId: string) => enrichActivity(volumeId),
}))

function render(props: { volumeId?: string; folderPath?: string } = {}): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FolderIndexStatus, {
    target,
    props: { volumeId: 'root', folderPath: '/Users/dave/Photos', ...props },
  })
  flushSync()
  return target
}

describe('FolderIndexStatus', () => {
  beforeEach(() => {
    settings['mediaIndex.enabled'] = true
    settings['mediaIndex.scope'] = 'chosen'
    settings['mediaIndex.alwaysIndexFolders'] = []
    settings['mediaIndex.excludedFolders'] = []
    enrichActivity.mockReturnValue(undefined)
    document.body.innerHTML = ''
  })

  it('renders nothing while image search is off', () => {
    settings['mediaIndex.enabled'] = false
    expect(render().textContent.trim()).toBe('')
  })

  it('renders nothing for a pane with no OS path', () => {
    expect(render({ folderPath: '' }).textContent.trim()).toBe('')
  })

  it('voices a folder the settings do not cover as not indexed', async () => {
    const target = render()
    expect(target.textContent).toContain('not indexed')
    await expectNoA11yViolations(target)
  })

  it('voices a chosen folder as indexed, and a running pass as indexing', async () => {
    settings['mediaIndex.alwaysIndexFolders'] = ['/Users/dave/Photos']
    const indexed = render()
    expect(indexed.textContent).toContain('Images indexed')
    await expectNoA11yViolations(indexed)

    document.body.innerHTML = ''
    enrichActivity.mockReturnValue({ volumeId: 'root', done: 5, total: 10, paused: null })
    const indexing = render()
    expect(indexing.textContent).toContain('Indexing images')
    await expectNoA11yViolations(indexing)
  })

  it('voices an excluded folder, even when it is also chosen', async () => {
    settings['mediaIndex.alwaysIndexFolders'] = ['/Users/dave/Photos']
    settings['mediaIndex.excludedFolders'] = ['/Users/dave/Photos']
    const target = render()
    expect(target.textContent).toContain('excluded')
    await expectNoA11yViolations(target)
  })

  it('never claims coverage it cannot prove in the automatic scope', async () => {
    settings['mediaIndex.scope'] = 'importance'
    const target = render()
    expect(target.textContent).toContain('automatically')
    await expectNoA11yViolations(target)
  })
})
