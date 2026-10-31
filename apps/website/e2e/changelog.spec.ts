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

    // A raw URL anywhere in the visible text means a link was rendered as text. Any owner
    // matches, so the guard keeps guarding if the repo moves to the `getcmdr` org (#476).
    expect(body).not.toMatch(/https:\/\/github\.com\/[A-Za-z0-9-]+\/cmdr\/commit\//)
    // Prose that merely ends in a parenthetical must survive untouched.
    expect(body).toContain('(~40x speed-up!)')
  })

  test('gives every release heading a `v<version>` anchor and keeps its visible text', async ({ page }) => {
    await page.goto('/changelog')

    const headings = await page.$$eval('.changelog-content h2', (nodes) =>
      nodes.map((node) => ({ id: node.id, text: (node.textContent ?? '').trim() })),
    )

    expect(headings.length).toBeGreaterThan(0)
    for (const heading of headings) {
      const version = /^\[(\d+\.\d+\.\d+)\] - \d{4}-\d{2}-\d{2}$/.exec(heading.text)?.[1]
      expect(version, `visible heading "${heading.text}"`).toBeDefined()
      expect(heading.id).toBe(`v${version}`)
    }
    expect(new Set(headings.map((heading) => heading.id)).size).toBe(headings.length)

    // The app's security notices link to this exact anchor from already shipped builds.
    await expect(page.locator('h2#v0\\.58\\.0')).toHaveText(/^\[0\.58\.0\] - 2026-10-31$/)
  })

  test('a jump to a release anchor leaves the heading visible below the fixed header', async ({ page }) => {
    // Reduced motion turns off the page's smooth scrolling, so the jump lands at once.
    await page.emulateMedia({ reducedMotion: 'reduce' })
    // An old release, so the jump has to scroll far from the top of the page.
    await page.goto('/changelog#v0.3.0')

    const position = await page.evaluate(() => ({
      headingTop: document.getElementById('v0.3.0')?.getBoundingClientRect().top ?? -1,
      headerBottom: document.querySelector('body header')?.getBoundingClientRect().bottom ?? -1,
      scrollY: window.scrollY,
    }))

    expect(position.scrollY).toBeGreaterThan(0)
    expect(position.headerBottom).toBeGreaterThan(0)
    expect(position.headingTop).toBeGreaterThanOrEqual(position.headerBottom)
    // Not merely clear of the header: it sits right under it, with no overshoot.
    expect(position.headingTop).toBeLessThan(position.headerBottom + 64)
  })
})
