/**
 * The formatted Markdown view focuses its scroll container once a render lands, so the
 * arrows and Page keys scroll it. That focus is the view's own doing (often right after
 * the user pressed `0`), so it must not paint the focus ring: WebKit matches
 * `:focus-visible` for a programmatic focus that follows a key press, which drew a
 * window-sized outline (#761). A later Tab focus still shows the ring.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MediaMarkdownView from './MediaMarkdownView.svelte'
import { QUIET_FOCUS_ATTRIBUTE } from './quiet-focus'

vi.mock('#lib/tauri-commands/index.js', () => ({
  viewerRenderMarkdown: vi.fn(() => Promise.resolve({ html: '<p>Hello</p>', hasRemoteImages: false })),
  viewerAllowRemoteImages: vi.fn(() => Promise.resolve()),
}))

async function mountRendered(): Promise<{ scroll: HTMLElement; destroy: () => void }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(MediaMarkdownView, {
    target,
    props: { sessionId: 'md-test', fileName: 'notes.md', landing: null, onFollowLink: () => {} },
  })
  let scroll: HTMLElement | null = null
  await vi.waitFor(async () => {
    await tick()
    scroll = target.querySelector<HTMLElement>('.markdown-scroll')
    expect(scroll).not.toBeNull()
    expect(document.activeElement).toBe(scroll)
  })
  return {
    scroll: scroll as unknown as HTMLElement,
    destroy: () => {
      void unmount(instance)
      target.remove()
    },
  }
}

describe('MediaMarkdownView focus', () => {
  it('focuses the document after rendering without the focus ring', async () => {
    const { scroll, destroy } = await mountRendered()
    await tick()
    expect(scroll.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(true)
    destroy()
  })

  it('brings the ring back once focus leaves, so a later Tab focus shows it', async () => {
    const { scroll, destroy } = await mountRendered()
    scroll.blur()
    await tick()
    expect(scroll.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(false)
    scroll.focus()
    await tick()
    expect(scroll.hasAttribute(QUIET_FOCUS_ATTRIBUTE)).toBe(false)
    destroy()
  })
})
