/**
 * The PDF view's Edit > Copy and Select all. Its text and selection live inside WebKit's
 * PDF embed, out of reach of the page and of the viewer's own selection model, so the view
 * hands both picks to the native responder chain, the only path that reaches the embed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { ViewerEditAction } from '#lib/ipc/bindings.js'

const { listeners, forwarded } = vi.hoisted(() => ({
  listeners: new Set<(payload: ViewerEditAction) => void>(),
  forwarded: [] as string[],
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  onViewerEditAction: (handler: (payload: ViewerEditAction) => void) => {
    listeners.add(handler)
    return Promise.resolve(() => listeners.delete(handler))
  },
  viewerForwardEditActionToPage: (action: string) => {
    forwarded.push(action)
    return Promise.resolve()
  },
}))

import MediaPdfView from './MediaPdfView.svelte'

async function mountView() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const view = mount(MediaPdfView, { target, props: { src: 'cmdr-media://localhost/t', fileName: 'a.pdf' } })
  await tick()
  await Promise.resolve()
  return () => {
    void unmount(view)
    target.remove()
  }
}

const pick = (action: ViewerEditAction['action']) => {
  for (const listener of listeners) listener({ action })
}

describe('MediaPdfView edit actions', () => {
  beforeEach(() => {
    listeners.clear()
    forwarded.length = 0
  })

  it('hands Copy and Select all to the native responder chain, which reaches the embed', async () => {
    const destroy = await mountView()

    pick('copy')
    pick('selectAll')

    expect(forwarded).toEqual(['copy', 'selectAll'])
    destroy()
  })

  it('stops forwarding once the PDF is off screen', async () => {
    const destroy = await mountView()
    destroy()
    await Promise.resolve()

    pick('copy')

    expect(forwarded).toEqual([])
  })
})
