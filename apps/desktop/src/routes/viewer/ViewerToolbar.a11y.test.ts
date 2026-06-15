import { describe, it, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

import ViewerToolbar from './ViewerToolbar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { EncodingChoice } from '$lib/ipc/bindings'

const choices: EncodingChoice[] = [
  { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
  { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

function mountToolbar(props: {
  isIndexing: boolean
  tailMode: boolean
  kind?: 'text' | 'image' | 'pdf'
  lastMediaKind?: 'text' | 'image' | 'pdf' | null
}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ViewerToolbar, {
    target,
    props: {
      fileName: 'example.txt',
      kind: props.kind ?? 'text',
      lastMediaKind: props.lastMediaKind ?? null,
      currentEncoding: 'utf8',
      detectedEncoding: 'utf8',
      encodingChoices: choices,
      isIndexing: props.isIndexing,
      tailMode: props.tailMode,
      onViewAsText: () => {},
      onViewAsMedia: () => {},
      onEncodingChange: () => {},
      onToggleTail: () => {},
    },
  })
  return target
}

describe('ViewerToolbar a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: false, tailMode: false })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('indexing + tail-on state has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: true, tailMode: true })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('media mode (image) has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: false, tailMode: false, kind: 'image' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('media file read as text (reverse-switch picker) has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: false, tailMode: false, kind: 'text', lastMediaKind: 'image' })
    await tick()
    await expectNoA11yViolations(target)
  })
})
