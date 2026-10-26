import { test, expect } from '@playwright/test'

test.describe('Homepage', () => {
  test('has correct title', async ({ page }) => {
    await page.goto('/')
    await expect(page).toHaveTitle(/Cmdr/i)
  })

  test('displays hero section with main heading', async ({ page }) => {
    await page.goto('/')

    // Check for main heading - matches actual site content
    const heading = page.locator('h1')
    await expect(heading).toBeVisible()
    await expect(heading).toContainText(/file manager/i)
  })

  test('has download CTA button', async ({ page }) => {
    await page.goto('/')

    // Look for at least one download button - use first() to avoid strict mode violation
    const ctaButton = page.getByRole('link', { name: /download/i }).first()
    await expect(ctaButton).toBeVisible()
  })

  test('displays pricing information', async ({ page }) => {
    await page.goto('/')

    // Check for pricing mention ($59 once, or free for personal use)
    const pageContent = await page.textContent('body')
    expect(pageContent).toMatch(/\$59|free.*personal/i)
  })

  test('every landing clip renders as a video, not a placeholder', async ({ page }) => {
    // A `public/clips/clips.json` shape the build can't read turns every clip into a placeholder
    // card without failing anything, so this is the guard for the manifest contract.
    await page.goto('/')
    await expect(page.locator('.clip-placeholder')).toHaveCount(0)
    const clips = page.locator('[data-product-clip]')
    await expect(clips).toHaveCount(6)
    for (const description of await clips.locator('[data-clip-description]').all()) {
      await expect(description).toHaveText(/\w/)
    }
  })

  test('a landing clip seeks and toggles from the keyboard', async ({ page }) => {
    // Reduced motion keeps the clip from autoplaying, so the position only moves when we move it.
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.goto('/')
    const clip = page.locator('[data-product-clip]').first()
    const slider = clip.getByRole('slider', { name: 'Clip position' })
    const max = Number(await slider.getAttribute('aria-valuemax'))
    expect(max).toBeGreaterThan(3)

    await slider.focus()
    await page.keyboard.press('End')
    await expect(slider).toHaveAttribute('aria-valuenow', String(max))
    await page.keyboard.press('Home')
    await expect(slider).toHaveAttribute('aria-valuenow', '0')
    await page.keyboard.press('PageUp')
    await expect(slider).toHaveAttribute('aria-valuenow', '5')
    await page.keyboard.press('ArrowLeft')
    await expect(slider).toHaveAttribute('aria-valuenow', '4')

    await page.keyboard.press('Space')
    await expect(clip).toHaveAttribute('data-state', 'playing')
    await page.keyboard.press('Space')
    await expect(clip).toHaveAttribute('data-state', 'paused')
  })

  test('dragging a landing clip scrubs it, and a plain click toggles it', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.goto('/')
    const clip = page.locator('[data-product-clip]').first()
    const slider = clip.getByRole('slider', { name: 'Clip position' })
    await slider.scrollIntoViewIfNeeded()
    const box = await slider.boundingBox()
    if (!box) throw new Error('The clip slider has no box')
    const y = box.y + box.height / 2
    const currentTime = () => clip.locator('video').evaluate((v: HTMLVideoElement) => v.currentTime)

    // Paused before the drag, so paused after it, a quarter of the clip further along.
    await page.mouse.move(box.x + box.width / 4, y)
    await page.mouse.down()
    await page.mouse.move(box.x + box.width / 2, y, { steps: 5 })
    await expect(clip).toHaveAttribute('data-scrubbing', '')
    await page.mouse.up()
    await expect(clip).not.toHaveAttribute('data-scrubbing')
    await expect(clip).toHaveAttribute('data-state', 'paused')
    const afterForward = await currentTime()
    expect(afterForward).toBeGreaterThan(1)

    // Dragging left goes back.
    await page.mouse.move(box.x + box.width / 2, y)
    await page.mouse.down()
    await page.mouse.move(box.x + box.width / 3, y, { steps: 5 })
    await page.mouse.up()
    await expect.poll(currentTime).toBeLessThan(afterForward)

    // A click without travel plays; a drag while playing pauses only for its length.
    await slider.click()
    await expect(clip).toHaveAttribute('data-state', 'playing')
    await page.mouse.move(box.x + box.width / 2, y)
    await page.mouse.down()
    await page.mouse.move(box.x + box.width * 0.6, y, { steps: 5 })
    await expect(clip).toHaveAttribute('data-state', 'paused')
    await page.mouse.up()
    await expect(clip).toHaveAttribute('data-state', 'playing')
  })

  test('testimonials name real users', async ({ page }) => {
    await page.goto('/')
    const section = page.locator('section', { has: page.getByRole('heading', { name: 'From early users' }) })
    await expect(section.locator('blockquote')).toHaveCount(3)
    await expect(section).toContainText('Sven Kopetzki')
    await expect(section).toContainText('Robert Benedict')
  })

  test('has navigation header', async ({ page }) => {
    await page.goto('/')

    // Check for essential nav elements
    const header = page.locator('header')
    await expect(header).toBeVisible()
  })

  test('is accessible (basic checks)', async ({ page }) => {
    await page.goto('/')

    // Check for alt text on images (if any exist)
    const images = await page.locator('img').all()
    for (const img of images) {
      const alt = await img.getAttribute('alt')
      expect(alt).not.toBeNull()
    }

    // Check for proper heading hierarchy (h1 exists)
    const h1Count = await page.locator('h1').count()
    expect(h1Count).toBeGreaterThanOrEqual(1)
  })
})
