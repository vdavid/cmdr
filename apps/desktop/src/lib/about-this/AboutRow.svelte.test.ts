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
    props: {
      card,
      onClose: () => {},
      onGoTo: () => {},
      onAct: () => {},
      onStopVerdict: () => {},
      aiHintDismissed: false,
      onDismissAiHint: () => {},
    },
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

function rowOf(target: HTMLElement, kind: string): HTMLElement {
  const found = target.querySelector<HTMLElement>(`[data-row-kind="${kind}"]`)
  if (!found) throw new Error(`no ${kind} row`)
  return found
}

describe('Kind', () => {
  // Regression: a confirmed JPEG read "Image, as its name says", the same words a claim would use.
  it('says Cmdr checked a match, and words a kind it read no bytes for as the name’s claim', async () => {
    expect(text(rowOf(await render(fixture('downloaded')), 'realType'))).toContain(
      'Disk image, checked against its contents',
    )
    if (mounted) void unmount(mounted)
    mounted = null
    document.body.innerHTML = ''
    expect(text(rowOf(await render(fixture('share-file')), 'realType'))).toMatch(/, as its name says$/)
  })
})

describe('A standard folder', () => {
  it('names the folder as the system’s and says what removing it means', async () => {
    const target = await render(fixture('standard-folder'))
    expect(text(rowOf(target, 'owner'))).toContain('Your Downloads folder, set up by the system')
    const whole = text(target)
    expect(whole).toContain('It’s one of your standard folders')
    expect(whole).not.toContain('Nothing Cmdr knows of made this')
    expect(whole).toContain('Cmdr hasn’t scored how much this folder matters yet')
  })
})
