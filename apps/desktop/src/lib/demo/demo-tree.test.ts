import { describe, expect, it } from 'vitest'
import { DEMO_HOME, DEMO_ROOT, demoEntries } from './demo-tree.js'

/** The folder holding `path`, or `/` for a top-level entry. */
function parentOf(path: string): string {
  const cut = path.lastIndexOf('/')
  return cut > 0 ? path.slice(0, cut) : '/'
}

describe('demo tree', () => {
  // The WASM engine refuses a tree with a missing parent or a duplicate (`tree.rs`), so a broken
  // tree here would fail the whole demo at boot, not one folder.
  it('lists every parent folder', () => {
    const folders = new Set([DEMO_ROOT, ...demoEntries.filter((e) => e.kind === 'dir').map((e) => e.path)])
    const orphans = demoEntries.filter((e) => !folders.has(parentOf(e.path))).map((e) => e.path)
    expect(orphans).toEqual([])
  })

  it('holds no path twice', () => {
    const paths = demoEntries.map((e) => e.path)
    expect(paths.length).toBe(new Set(paths).size)
  })

  it('spells every path absolute, without a trailing slash', () => {
    const bad = demoEntries.map((e) => e.path).filter((p) => !p.startsWith('/') || p.endsWith('/'))
    expect(bad).toEqual([])
  })

  it('has the home folder and its big photo folder of 1,200 files', () => {
    expect(demoEntries.some((e) => e.path === DEMO_HOME && e.kind === 'dir')).toBe(true)
    const photos = demoEntries.filter((e) => parentOf(e.path) === `${DEMO_HOME}/Pictures/Camera roll`)
    expect(photos).toHaveLength(1200)
    expect(photos.every((e) => e.kind === 'file' && (e.size ?? 0) > 0)).toBe(true)
  })

  it('has hidden files to toggle, and a non-Latin name for Brief widths', () => {
    expect(demoEntries.some((e) => parentOf(e.path) === DEMO_HOME && e.path.split('/').pop()?.startsWith('.'))).toBe(
      true,
    )
    expect(demoEntries.some((e) => /[^ -~]/u.test(e.path))).toBe(true)
  })
})
