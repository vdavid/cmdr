import { test, expect } from '@playwright/test'

// `CHANGELOG.md` stores each entry's commit refs as a bare trailing group,
// `- Some change (b626d7a4, 2d41cc14)`. The page linkifies them at build time
// (src/lib/changelog.ts), so the rendered HTML must carry real anchors even
// though the source markdown has none. These assertions stay generic on purpose:
// the changelog churns every release.

test.describe('Changelog', () => {
  test('renders bare commit hashes as GitHub links', async ({ page }) => {
    await page.goto('/changelog')
    const commitLinks = page.locator('a[href^="https://github.com/vdavid/cmdr/commit/"]')

    expect(await commitLinks.count()).toBeGreaterThan(0)

    // Every commit link shows the bare hash and points at that same hash.
    for (const link of await commitLinks.all()) {
      const text = ((await link.textContent()) ?? '').trim()
      expect(text).toMatch(/^[0-9a-f]{6,40}$/)
      expect(await link.getAttribute('href')).toBe(`https://github.com/vdavid/cmdr/commit/${text}`)
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
