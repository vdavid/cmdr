/**
 * A picture's "What it shows" row, over the gallery's fixture cards: what the image index recognized, each reason it
 * has nothing, and that the picture's own text stays a text node.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import AboutThisDialog from './AboutThisDialog.svelte'
import type { AboutCardData } from './about-card'
import { aboutThisFixtures } from '#lib/dialog-gallery/fixtures/about-this.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))

let mounted: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
})

function fixture(id: string): AboutCardData {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  return card
}

async function render(card: AboutCardData): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutThisDialog, {
    target,
    props: { card, onClose: () => {}, onGoTo: () => {}, onAct: () => {}, onStopVerdict: () => {} },
  })
  flushSync()
  await tick()
  return target
}

function row(target: HTMLElement): HTMLElement {
  const found = target.querySelector<HTMLElement>('[data-row-kind="imageContents"]')
  if (!found) throw new Error('no What it shows row')
  return found
}

function text(element: Element): string {
  return element.textContent.replace(/\s+/g, ' ').trim()
}

describe('What it shows', () => {
  it('names the tags and quotes the first lines of text, saying more follows', async () => {
    const shown = text(row(await render(fixture('picture-shows'))))
    expect(shown).toContain('What it shows')
    expect(shown).toContain('Tagged as sign, text, outdoor, and storefront')
    expect(shown).toContain('Text in it:')
    expect(shown).toContain('“Corner Bakery”')
    expect(shown).toContain('“Fresh bread from 7”')
    expect(shown).toContain('And more text')
  })

  it('says in one quiet line why there is nothing: not indexed, indexing off, or nothing found', async () => {
    for (const [id, line] of [
      ['picture-not-indexed', 'Not in Cmdr’s image index'],
      ['picture-indexing-off', 'Image indexing is off'],
      ['picture-nothing-found', 'No tags or text found'],
    ] as const) {
      const target = await render(fixture(id))
      expect(text(row(target))).toBe(`What it shows ${line}`)
      if (mounted) void unmount(mounted)
      mounted = null
      document.body.innerHTML = ''
    }
  })

  it('renders the picture’s own text as text, never markup', async () => {
    const card = fixture('picture-shows')
    const hostile: AboutCardData = {
      ...card,
      facts: {
        ...card.facts,
        imageContents: {
          kind: 'imageContents',
          fact: {
            state: 'known',
            value: { tags: ['<b>sign</b>'], textLines: ['<img src=x onerror=alert(1)>'], moreText: false },
          },
        },
      },
    }
    const target = row(await render(hostile))
    expect(target.querySelector('img')).toBeNull()
    expect(target.querySelector('b')).toBeNull()
    expect(text(target)).toContain('<img src=x onerror=alert(1)>')
    expect(text(target)).toContain('<b>sign</b>')
  })
})
