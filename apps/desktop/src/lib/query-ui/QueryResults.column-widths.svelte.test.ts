/**
 * End-to-end wiring for the results table's column widths in `QueryResults.svelte` over
 * `$lib/ui/ColumnList.svelte`.
 *
 * The math itself is pinned in `$lib/ui/column-list-layout.test.ts` and the declarations in
 * `result-column-widths.test.ts`; this file pins that the
 * component actually reaches it and writes ONE template onto both grid containers. jsdom
 * has no Canvas 2D, so `@chenglou/pretext` is stubbed with a 10px-per-character font, and
 * no layout either, so the container's `clientWidth` is stubbed to a fixed width.
 */

import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchResults from './QueryResults.svelte'
import type { SearchResultEntry } from '$lib/tauri-commands'

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return {
    getCachedIcon: () => undefined,
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
  }
})

vi.mock('$lib/tauri-commands', () => ({}))

const CHAR_PX = 10
const CONTAINER_PX = 600

vi.mock('@chenglou/pretext', () => ({
  prepareWithSegments: (text: string) => ({ text }),
  measureNaturalWidth: (prepared: { text: string }) => prepared.text.length * CHAR_PX,
}))

const clientWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth')
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', {
    configurable: true,
    get(this: HTMLElement) {
      return this.classList.contains('column-list-viewport') ? CONTAINER_PX : 0
    },
  })
})
afterAll(() => {
  if (clientWidth) Object.defineProperty(HTMLElement.prototype, 'clientWidth', clientWidth)
})

const baseProps = {
  results: [] as SearchResultEntry[],
  cursorIndex: -1,
  isIndexAvailable: true,
  isIndexReady: true,
  isSearching: false,
  hasSearched: true,
  query: '*',
  sizeFilter: 'any',
  dateFilter: 'any',
  scanning: false,
  entriesScanned: 0,
  totalCount: 0,
  indexEntryCount: 1000,
  countOnly: false,
  showPathColumn: true,
  onShowResults: undefined as (() => void) | undefined,
  iconCacheVersion: 0,
  aiEnabled: false,
  onResultClick: () => {},
  onHover: () => {},
  onPickExample: () => {},
  onPickPath: () => {},
  onRowMenu: () => {},
}

function entry(name: string, dir: string): SearchResultEntry {
  return {
    path: `${dir}/${name}`,
    name,
    parentPath: dir,
    isDirectory: false,
    size: 1,
    modifiedAt: 0,
    iconId: 'ext:txt',
  }
}

/** Mounts, then lets the dynamic `import('@chenglou/pretext')` and its effects settle. */
async function mountAndSettle(results: SearchResultEntry[], showPathColumn = true): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SearchResults, { target, props: { ...baseProps, results, totalCount: results.length, showPathColumn } })
  for (let i = 0; i < 6; i++) {
    await tick()
    await Promise.resolve()
  }
  return target
}

/** `[icon, name, path, size, modified]` tracks off the header's inline template. */
function tracks(target: HTMLDivElement): string[] {
  const header = target.querySelector<HTMLElement>('.column-header')
  // Split on spaces outside `minmax(…)`.
  return header?.style.gridTemplateColumns.match(/minmax\([^)]*\)|\S+/g) ?? []
}

describe('QueryResults column widths', () => {
  it('lets long names take the width a short path leaves over', async () => {
    // David's case: long filenames in "~/Downloads", which the old 22ch cap cut short.
    const long = '2026-09-25 lönebesked september.pdf'
    const target = await mountAndSettle([entry(long, '~/Downloads'), entry('other.pdf', '~/Downloads')])
    const [, name, path] = tracks(target)
    expect(name).toMatch(/^minmax\(\d+px, 1fr\)$/)
    expect(path).toMatch(/^\d+px$/)
    // The path strip is far narrower than the name, so Name isn't capped anywhere near 22ch.
    expect(parseInt(path, 10)).toBeLessThan(long.length * CHAR_PX)
  })

  it('shrink-wraps short names and hands the rest to a long path', async () => {
    const deep = '/opt/homebrew/lib/python3.13/site-packages/some-package/deeply/nested/folder'
    const target = await mountAndSettle([entry('test', deep), entry('test', '/usr/local/lib')])
    const [, name, path] = tracks(target)
    expect(name).toMatch(/^\d+px$/)
    expect(path).toMatch(/^minmax\(\d+px, 1fr\)$/)
  })

  it('measures Size and Modified to fixed pixel tracks', async () => {
    const target = await mountAndSettle([entry('one.txt', '~')])
    const [icon, , , size, modified] = tracks(target)
    expect(icon).toBe('24px')
    expect(size).toMatch(/^\d+px$/)
    expect(modified).toMatch(/^\d+px$/)
  })

  it('keeps the header and the rows on the same measured template', async () => {
    const target = await mountAndSettle([entry('one.txt', '~'), entry('two-longer-name.txt', '~/Documents')])
    const header = target.querySelector<HTMLElement>('.column-header')
    const rows = target.querySelectorAll<HTMLElement>('.result-row')
    expect(rows.length).toBe(2)
    for (const row of rows) {
      expect(row.style.gridTemplateColumns).toBe(header?.style.gridTemplateColumns)
    }
  })

  it('keeps Name as the flex track when there is no Path column', async () => {
    const target = await mountAndSettle([entry('one.txt', '~')], false)
    expect(tracks(target)).toEqual(['24px', 'minmax(80px, 1fr)', '10ch', '16ch'])
  })
})
