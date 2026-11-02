import { describe, it, expect, afterEach } from 'vitest'
import {
  CONTENT_FADE_BAND_PX,
  contentScrollTopFor,
  findSectionElements,
  flashElements,
  sectionDomId,
} from './settings-navigation'
import { buildSectionTree, type SettingsSection } from './settings-registry'

function leafPaths(sections: SettingsSection[]): string[][] {
  return sections.flatMap((s) => (s.subsections.length > 0 ? leafPaths(s.subsections) : [s.path]))
}

function sectionsDom(ids: string[]): HTMLElement {
  const container = document.createElement('div')
  for (const id of ids) {
    const section = document.createElement('section')
    section.dataset.sectionId = id
    container.appendChild(section)
  }
  document.body.appendChild(container)
  return container
}

function rect(top: number): DOMRect {
  return { top } as DOMRect
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('sectionDomId', () => {
  // Regression: a sidebar click built `updates---privacy` while the page rendered `updates`,
  // so five sections silently ignored every click.
  it.each([
    [['AI', 'MCP server'], 'ai-mcp-server'],
    [['Updates & privacy'], 'updates-and-privacy'],
    [['Behavior', 'Navigation & file ops'], 'behavior-navigation-and-file-ops'],
    [['File systems', 'Servers (SFTP, WebDAV)'], 'file-systems-servers-sftp-webdav'],
    [['File systems', 'MTP (Android/Kindle/cameras)'], 'file-systems-mtp-android-kindle-cameras'],
    [['File systems', 'Android (ADB)'], 'file-systems-android-adb'],
  ])('maps %j to %s', (path, id) => {
    expect(sectionDomId(path)).toBe(id)
  })

  it('gives every section in the tree a distinct id', () => {
    const ids = leafPaths(buildSectionTree()).map(sectionDomId)
    expect(new Set(ids).size).toBe(ids.length)
  })

  // `findSectionElements` treats `<id>-…` as a subsection, so one leaf can't prefix another.
  it('never makes one leaf look like a subsection of another', () => {
    const ids = leafPaths(buildSectionTree()).map(sectionDomId)
    expect(ids.filter((id) => ids.some((other) => other.startsWith(`${id}-`)))).toEqual([])
  })
})

describe('findSectionElements', () => {
  it('finds a leaf section', () => {
    const container = sectionsDom(['ai-provider', 'ai-mcp-server'])
    expect(findSectionElements(container, ['AI', 'MCP server']).map((el) => el.dataset.sectionId)).toEqual([
      'ai-mcp-server',
    ])
  })

  it('finds every rendered subsection of a parent, in page order', () => {
    const container = sectionsDom(['appearance-listing', 'ai-ask-cmdr', 'ai-mcp-server', 'viewer'])
    expect(findSectionElements(container, ['AI']).map((el) => el.dataset.sectionId)).toEqual([
      'ai-ask-cmdr',
      'ai-mcp-server',
    ])
  })

  it('returns nothing when no section is rendered', () => {
    expect(findSectionElements(sectionsDom(['viewer']), ['AI'])).toEqual([])
  })
})

describe('contentScrollTopFor', () => {
  function scroller(scrollTop: number, top: number): HTMLElement {
    const el = document.createElement('div')
    el.scrollTop = scrollTop
    el.getBoundingClientRect = () => rect(top)
    return el
  }
  function target(top: number): HTMLElement {
    const el = document.createElement('div')
    el.getBoundingClientRect = () => rect(top)
    return el
  }

  // Regression: the old `offsetTop - 16` parked headings 8 px below the top edge, inside the
  // fade's fully transparent band.
  it('lands the target just below the top fade', () => {
    expect(contentScrollTopFor(scroller(100, 20), target(520))).toBe(100 + 500 - CONTENT_FADE_BAND_PX)
  })

  it('never goes above the top', () => {
    expect(contentScrollTopFor(scroller(0, 20), target(50))).toBe(0)
  })
})

describe('flashElements', () => {
  it('adds the flash class and removes it when the animation ends', () => {
    const el = document.createElement('div')
    flashElements([el])
    expect(el.classList.contains('settings-flash')).toBe(true)
    el.dispatchEvent(new Event('animationend'))
    expect(el.classList.contains('settings-flash')).toBe(false)
  })

  it('a replay keeps the class until its own animation ends', () => {
    const el = document.createElement('div')
    flashElements([el])
    flashElements([el])
    expect(el.classList.contains('settings-flash')).toBe(true)
    el.dispatchEvent(new Event('animationend'))
    expect(el.classList.contains('settings-flash')).toBe(false)
    flashElements([el])
    expect(el.classList.contains('settings-flash')).toBe(true)
  })
})
