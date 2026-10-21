import { describe, it, expect } from 'vitest'

import {
  clickedLinkHref,
  findAnchorTarget,
  loadRemoteImages,
  parseImageMarker,
  wireImages,
  type PlaceholderLabels,
} from './markdown-view'
import { mediaUrl } from './media-view'

const LABELS: PlaceholderLabels = { remote: 'From the web', blocked: 'Not shown', missing: 'Not found' }
const TOKEN = '0123456789abcdef0123456789abcdef'

function article(html: string): HTMLElement {
  const root = document.createElement('article')
  root.innerHTML = html
  return root
}

function placeholders(root: HTMLElement): string[] {
  return [...root.querySelectorAll('[data-md-placeholder]')].map((el) => el.textContent.trim())
}

describe('parseImageMarker', () => {
  it('reads the three marker shapes', () => {
    expect(parseImageMarker(`local:${TOKEN}`)).toEqual({ kind: 'local', token: TOKEN })
    expect(parseImageMarker(`remote:${TOKEN}`)).toEqual({ kind: 'remote', token: TOKEN })
    expect(parseImageMarker('blocked')).toEqual({ kind: 'blocked' })
  })

  it('treats anything else as blocked', () => {
    expect(parseImageMarker(null)).toEqual({ kind: 'blocked' })
    expect(parseImageMarker('local:')).toEqual({ kind: 'blocked' })
    expect(parseImageMarker('other:abc')).toEqual({ kind: 'blocked' })
  })
})

describe('wireImages', () => {
  it('loads a local image from its media URL', () => {
    const root = article(`<p><img data-md-image="local:${TOKEN}" alt="Shot"></p>`)
    wireImages(root, LABELS)
    const img = root.querySelector('img')
    expect(img?.getAttribute('src')).toBe(mediaUrl(TOKEN))
    expect(img?.hidden).toBe(false)
    expect(placeholders(root)).toEqual([])
  })

  it('holds a remote image back behind a placeholder naming it', () => {
    const root = article(`<img data-md-image="remote:${TOKEN}" alt="Build status">`)
    wireImages(root, LABELS)
    const img = root.querySelector('img')
    expect(img?.hasAttribute('src')).toBe(false)
    expect(img?.hidden).toBe(true)
    expect(placeholders(root)).toEqual(['From the web: Build status'])
  })

  it('shows a blocked image as a placeholder, without its alt when it has none', () => {
    const root = article('<img data-md-image="blocked">')
    wireImages(root, LABELS)
    expect(placeholders(root)).toEqual(['Not shown'])
  })

  it('swaps a local image that does not load for a placeholder', () => {
    const root = article(`<img data-md-image="local:${TOKEN}" alt="Gone">`)
    wireImages(root, LABELS)
    root.querySelector('img')?.dispatchEvent(new Event('error'))
    expect(placeholders(root)).toEqual(['Not found: Gone'])
    expect(root.querySelector('img')?.hidden).toBe(true)
  })

  it('leaves an inline image alone', () => {
    const root = article('<img src="data:image/png;base64,iVBORw0KGgo=" alt="Dot">')
    wireImages(root, LABELS)
    expect(root.querySelector('img')?.getAttribute('src')).toBe('data:image/png;base64,iVBORw0KGgo=')
    expect(placeholders(root)).toEqual([])
  })
})

describe('loadRemoteImages', () => {
  it('puts held-back images in and leaves local ones as they were', () => {
    const root = article(
      `<img data-md-image="remote:${TOKEN}" alt="Badge"><img data-md-image="local:${TOKEN.replace('0', 'f')}">`,
    )
    wireImages(root, LABELS)
    const local = root.querySelectorAll('img')[1]
    const localSrc = local.getAttribute('src')

    loadRemoteImages(root, LABELS)
    const remote = root.querySelectorAll('img')[0]
    expect(remote.getAttribute('src')).toBe(mediaUrl(TOKEN))
    expect(remote.hidden).toBe(false)
    expect(placeholders(root)).toEqual([])
    expect(local.getAttribute('src')).toBe(localSrc)
  })
})

describe('clickedLinkHref', () => {
  it('finds the link a click landed in, through nested markup', () => {
    const root = article('<p><a href="setup.md#install"><strong>Set up</strong></a> and more</p>')
    expect(clickedLinkHref(root.querySelector('strong'))).toBe('setup.md#install')
  })

  it('is null off a link, or on a link with no href (the sanitizer dropped it)', () => {
    const root = article('<p>plain <a>no target</a></p>')
    expect(clickedLinkHref(root.querySelector('p'))).toBeNull()
    expect(clickedLinkHref(root.querySelector('a'))).toBeNull()
    expect(clickedLinkHref(null)).toBeNull()
  })
})

describe('findAnchorTarget', () => {
  it('finds the element with that id inside the document, whatever characters it holds', () => {
    const root = article('<h2 id="md-árvíz">A</h2><h2 id="md-a.b:c">B</h2>')
    expect(findAnchorTarget(root, 'md-árvíz')?.textContent).toBe('A')
    expect(findAnchorTarget(root, 'md-a.b:c')?.textContent).toBe('B')
  })

  it('is null for an id the document doesn’t have', () => {
    expect(findAnchorTarget(article('<h2 id="md-a">A</h2>'), 'md-b')).toBeNull()
  })
})
