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

  test('every recorded landing clip renders as a video, not a placeholder', async ({ page }) => {
    // A `public/clips/clips.json` shape the build can't read turns every clip into a placeholder
    // card without failing anything, so this is the guard for the manifest contract. Only the two
    // reach clips may still be placeholders while they wait to be recorded.
    const notRecordedYet = ['reach-network', 'reach-devices']
    await page.goto('/')
    const placeholders = await page
      .locator('[data-clip-placeholder]')
      .evaluateAll((els) => els.map((el) => el.getAttribute('data-clip-placeholder')))
    for (const slug of placeholders) expect(notRecordedYet).toContain(slug)
    const clips = page.locator('[data-product-clip]')
    await expect(clips).toHaveCount(8 - placeholders.length)
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

  test('key captions follow the playback position, and an invalid keys entry is dropped', async ({ page }) => {
    // `/clip-fixture` renders one clip with `src/fixtures/clip-keys.json`: F5 at 1 s, ⏎ at 1.3 s,
    // ⌘Z at 5 s, a long typed string at 8 s, plus two invalid entries. Reduced motion stops autoplay and the fade, so each
    // seek gives a still, exact state.
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.goto('/clip-fixture')
    const clip = page.locator('[data-product-clip]')
    const slider = clip.getByRole('slider', { name: 'Clip position' })
    const captions = clip.locator('[data-clip-keys]')
    await expect(captions).toHaveAttribute('aria-hidden', 'true')
    const seek = (seconds: number) =>
      clip.locator('video').evaluate((v: HTMLVideoElement, s) => {
        v.currentTime = s
      }, seconds)

    // Home loads the clip; then seek to moments around the presses.
    await slider.focus()
    await page.keyboard.press('Home')
    await expect(clip).toHaveAttribute('data-ready', '')
    await seek(1.5)
    await expect(captions.locator('.clip-key')).toHaveText(['F5', '⏎'])
    await seek(2.8)
    await expect(captions.locator('.clip-key')).toHaveText(['⏎'])
    await seek(3.5)
    await expect(captions.locator('.clip-key')).toHaveCount(0)
    await seek(5.2)
    await expect(captions.locator('.clip-key')).toHaveText(['⌘Z'])
    // ⌘Z is gone 1.6 s after its press, and the blank label at 6 s and the non-numeric one never show.
    await seek(6.8)
    await expect(captions.locator('.clip-key')).toHaveCount(0)
    // Back to the start, as a loop would: the early presses come back.
    await seek(1.1)
    await expect(captions.locator('.clip-key')).toHaveText(['F5'])
  })

  test('a long typed-string caption stays inside a phone-width clip', async ({ page }) => {
    // A typed string is one cap (docs/guides/product-clips.md § Key captions); the fixture's at 8 s is long.
    await page.setViewportSize({ width: 390, height: 844 })
    await page.emulateMedia({ reducedMotion: 'reduce' })
    await page.goto('/clip-fixture')
    const clip = page.locator('[data-product-clip]')
    await clip.getByRole('slider', { name: 'Clip position' }).focus()
    await page.keyboard.press('Home')
    await expect(clip).toHaveAttribute('data-ready', '')
    await clip.locator('video').evaluate((v: HTMLVideoElement) => {
      v.currentTime = 8.2
    })
    const cap = clip.locator('.clip-key')
    await expect(cap).toHaveCount(1)
    const clipBox = await clip.boundingBox()
    const capBox = await cap.boundingBox()
    if (!clipBox || !capBox) throw new Error('The clip or its caption has no box')
    expect(capBox.x).toBeGreaterThanOrEqual(clipBox.x)
    expect(capBox.x + capBox.width).toBeLessThanOrEqual(clipBox.x + clipBox.width)
  })

  test('testimonials name real users', async ({ page }) => {
    await page.goto('/')
    const section = page.getByRole('region', { name: 'Testimonials' })
    // Every quote is in the HTML (toContainText reads hidden cards too); three show at a time.
    await expect(section.locator('blockquote')).toHaveCount(5)
    await expect(section.locator('blockquote:visible')).toHaveCount(3)
    await expect(section).toContainText('Sven Kopetzki')
    await expect(section).toContainText('André Dammeyer')
    await expect(section).toContainText('Robert Benedict')
    await expect(section).toContainText('Random redditor')
    // A quote without the person's yes goes up without a name.
    await expect(section).toContainText('An early user')
    await expect(section).not.toContainText('Jojo')
    // A name links only where the person offered a profile, or the quote is a public comment.
    await expect(section.locator('figcaption a')).toHaveCount(2)
    // Avatars are local, pipeline-optimized copies, never hotlinks (Gravatar, LinkedIn, Reddit).
    const avatars = section.locator('img')
    await expect(avatars).toHaveCount(3)
    for (const avatar of await avatars.all()) {
      await expect(avatar).toHaveAttribute('src', /^\/_astro\/.+\.webp$/)
    }
  })

  test('testimonials show a different three across page loads', async ({ page }) => {
    // The pick is random, so compare loads: 60 possible orderings make 20 identical ones in a row
    // a practical impossibility, and the loop usually ends on the second load.
    const seen = new Set<string>()
    for (let load = 0; load < 20 && seen.size < 2; load++) {
      await page.goto('/')
      const visible = page.getByRole('region', { name: 'Testimonials' }).locator('blockquote:visible')
      await expect(visible).toHaveCount(3)
      seen.add((await visible.allTextContents()).join('|'))
    }
    expect(seen.size).toBeGreaterThan(1)
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
