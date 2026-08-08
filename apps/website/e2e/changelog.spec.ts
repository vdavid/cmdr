import { test, expect } from '@playwright/test'

// `CHANGELOG.md` stores each entry's commit refs as a bare trailing group,
// `- Some change (b626d7a4, 2d41cc14)`. The page linkifies them at build time
// (src/lib/changelog.ts), so the rendered HTML must carry real anchors even
// though the source markdown has none. These assertions stay generic on purpose:
// the changelog churns every release.

test.describe('Changelog', () => {
  test('renders bare commit hashes as GitHub links', async ({ page }) => {
    await page.goto('/changelog')

    // Pull every link's text and href in ONE evaluate, then assert in Node.
    // Looping `locator.textContent()` / `.getAttribute()` costs two round-trips
    // PER LINK, and the changelog only grows: at 1 606 links that loop blew the
    // 30 s test timeout. The assertions below are unchanged and still cover every
    // link — this only stops the test from racing the clock as we ship releases.
    const links = await page.$$eval('a[href^="https://github.com/vdavid/cmdr/commit/"]', (nodes) =>
      nodes.map((node) => ({ text: (node.textContent ?? '').trim(), href: node.getAttribute('href') ?? '' })),
    )

    expect(links.length).toBeGreaterThan(0)

    // Every commit link shows the bare hash and points at that same hash.
    for (const link of links) {
      expect(link.text).toMatch(/^[0-9a-f]{6,40}$/)
      expect(link.href).toBe(`https://github.com/vdavid/cmdr/commit/${link.text}`)
    }
  })

  test('leaves a hash group out of the visible text and prose parentheticals alone', async ({ page }) => {
    await page.goto('/changelog')
    const body = (await page.textContent('body')) ?? ''

    // A raw URL anywhere in the visible text means a link was rendered as text.
    expect(body).not.toContain('https://github.com/vdavid/cmdr/commit/')
    // Prose that merely ends in a parenthetical must survive untouched.
    expect(body).toContain('(~40x speed-up!)')
  })
})
