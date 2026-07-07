import { test, expect } from '@playwright/test'

// The build-time smart-quotes integration (src/plugins/smart-quotes.ts) must curl quotes
// regardless of how the text reached the HTML: a literal `"` (via set:html, e.g. the homepage
// feature list) or an HTML entity `&quot;`/`&#39;` (via Astro's `{text}` interpolation, e.g. the
// /features cards). Playwright runs against the built `dist`, so the integration has run.
//
// page.textContent decodes entities, so a straight ASCII quote in the assertions below means the
// integration missed it; a curly quote means it caught it.

const STRAIGHT = 'Find files like "that PDF contract from last month"'
const CURLY = 'Find files like “that PDF contract from last month”'

test.describe('Smart quotes', () => {
  test('/features curls quotes that reached the HTML as entities', async ({ page }) => {
    await page.goto('/features')
    const body = (await page.textContent('body')) ?? ''
    expect(body).toContain(CURLY)
    expect(body).not.toContain(STRAIGHT)
  })

  test('homepage curls quotes that reached the HTML literally (set:html path)', async ({ page }) => {
    await page.goto('/')
    const body = (await page.textContent('body')) ?? ''
    expect(body).toContain('“that PDF contract from last month”')
    expect(body).not.toContain('"that PDF contract from last month"')
  })

  test('homepage curls an apostrophe in plain .astro template text', async ({ page }) => {
    // "what's next" is a literal apostrophe in Download.astro's template (not markdown, not an
    // entity), so it exercises the word-apostrophe rule on the .astro template path.
    await page.goto('/')
    const body = (await page.textContent('body')) ?? ''
    expect(body).toContain('feedback shapes what’s next')
    expect(body).not.toContain("feedback shapes what's next")
  })
})
