import { test, expect } from '@playwright/test'

test.describe('Blog', () => {
    test('blog index loads with correct title', async ({ page }) => {
        await page.goto('/blog')
        await expect(page).toHaveTitle('Blog — Cmdr')
    })

    test('blog index lists posts with dates', async ({ page }) => {
        await page.goto('/blog')
        const article = page.locator('article').first()
        await expect(article).toBeVisible()
        await expect(article.locator('time')).toBeVisible()
    })

    test('blog index shows excerpt, not full post', async ({ page }) => {
        await page.goto('/blog')
        const article = page.locator('article').first()
        // Excerpt content should be visible
        await expect(article.locator('.blog-content')).toContainText('What to expect')
        // Content after the <!-- more --> marker should not appear
        await expect(article.locator('.blog-content')).not.toContainText('A quick look at Cmdr')
    })

    test('blog index has "Read more" link to full post', async ({ page }) => {
        await page.goto('/blog')
        const readMore = page.locator('.read-more-link').first()
        await expect(readMore).toBeVisible()
        await expect(readMore).toHaveAttribute('href', '/blog/hello-world')
        await readMore.click()
        await expect(page).toHaveURL(/\/blog\/hello-world/)
        await expect(page.locator('h1')).toContainText('Hello, world')
    })

    test('post title links to individual post page', async ({ page }) => {
        await page.goto('/blog')
        const postLink = page.locator('article a[href*="/blog/"]').first()
        await expect(postLink).toBeVisible()
        await postLink.click()
        await expect(page).toHaveURL(/\/blog\/hello-world/)
        await expect(page.locator('h1')).toContainText('Hello, world')
    })

    test('individual post shows full content including sections after excerpt', async ({ page }) => {
        await page.goto('/blog/hello-world')
        // Both excerpt and post-excerpt content should be visible
        await expect(page.locator('.blog-content')).toContainText('What to expect')
        await expect(page.locator('.blog-content')).toContainText('A quick look at Cmdr')
        await expect(page.locator('.blog-content')).toContainText('Built with modern tools')
    })

    test('individual post shows date and description', async ({ page }) => {
        await page.goto('/blog/hello-world')
        await expect(page.locator('main time')).toBeVisible()
        await expect(page.locator('main header p')).toContainText('Welcome to the Cmdr blog')
    })

    test('individual post has correct meta tags', async ({ page }) => {
        await page.goto('/blog/hello-world')
        const ogImage = page.locator('meta[property="og:image"]')
        await expect(ogImage).toHaveAttribute('content', /\/og\/hello-world\.png/)
        const description = page.locator('meta[name="description"]')
        await expect(description).toHaveAttribute('content', /Welcome to the Cmdr blog/)
        const ogTitle = page.locator('meta[property="og:title"]')
        await expect(ogTitle).toHaveAttribute('content', /Hello, world/)
    })

    test('individual post has comments section', async ({ page }) => {
        await page.goto('/blog/hello-world')
        await expect(page.locator('.comments-section')).toBeVisible()
        await expect(page.getByText('Comments')).toBeVisible()
    })

    test('external links open in new tabs', async ({ page }) => {
        await page.goto('/blog/hello-world')
        const externalLinks = page.locator('.blog-content a[target="_blank"]')
        const count = await externalLinks.count()
        expect(count).toBeGreaterThan(0)
        for (let i = 0; i < count; i++) {
            await expect(externalLinks.nth(i)).toHaveAttribute('rel', /noopener/)
        }
    })

    test('RSS feed returns valid XML with post data', async ({ request }) => {
        const response = await request.get('/rss.xml')
        expect(response.status()).toBe(200)
        expect(response.headers()['content-type']).toContain('xml')
        const body = await response.text()
        expect(body).toContain('<title>Cmdr blog</title>')
        expect(body).toContain('Hello, world')
        expect(body).toContain('/blog/hello-world/')
    })

    test('OG image returns PNG', async ({ request }) => {
        const response = await request.get('/og/hello-world.png')
        expect(response.status()).toBe(200)
        expect(response.headers()['content-type']).toContain('png')
    })

    test('navigation has Blog link', async ({ page }) => {
        await page.goto('/')
        const blogLink = page.getByRole('navigation').getByRole('link', { name: 'Blog' })
        await expect(blogLink).toBeVisible()
    })
})
