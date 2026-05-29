import { describe, it, beforeEach, vi } from 'vitest'
import { mount, tick } from 'svelte'

import ViewerStatusBar from './ViewerStatusBar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountStatusBar(props: {
  currentMode: 'fullLoad' | 'byteSeek' | 'lineIndex'
  isIndexing: boolean
  wordWrap: boolean
  totalLines: number | null
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ViewerStatusBar, {
    target,
    props: {
      fileName: 'example.txt',
      totalLines: props.totalLines,
      totalBytes: 2048,
      currentMode: props.currentMode,
      isIndexing: props.isIndexing,
      wordWrap: props.wordWrap,
      indexingTimeoutSecs: 5,
    },
  })
  return target
}

describe('ViewerStatusBar a11y', () => {
  it('in-memory state has no a11y violations', async () => {
    const target = mountStatusBar({ currentMode: 'fullLoad', isIndexing: false, wordWrap: false, totalLines: 42 })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('streaming + wrap + unknown line count has no a11y violations', async () => {
    const target = mountStatusBar({ currentMode: 'byteSeek', isIndexing: true, wordWrap: true, totalLines: null })
    await tick()
    await expectNoA11yViolations(target)
  })
})
