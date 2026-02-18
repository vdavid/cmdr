import { test, expect } from '@playwright/test'

const slug = 'cmdr-will-track-your-entire-file-system'
const title = 'Cmdr will track your entire file system!'

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
        await expect(article.locator('.blog-content')).toContainText('thinking about displaying dir sizes')
        // Content after the <!-- more --> marker should not appear
        await expect(article.locator('.blog-content')).not.toContainText('so excited')
    })

    test('blog index has "Read more" link to full post', async ({ page }) => {
        await page.goto('/blog')
        const readMore = page.locator('.read-more-link').first()
        await expect(readMore).toBeVisible()
        await expect(readMore).toHaveAttribute('href', `/blog/${slug}`)
        await readMore.click()
        await expect(page).toHaveURL(new RegExp(`/blog/${slug}`))
        await expect(page.locator('h1')).toContainText(title)
    })

    test('post title links to individual post page', async ({ page }) => {
        await page.goto('/blog')
        const postLink = page.locator(`article a[href*="/blog/${slug}"]`).first()
        await expect(postLink).toBeVisible()
        await postLink.click()
        await expect(page).toHaveURL(new RegExp(`/blog/${slug}`))
        await expect(page.locator('h1')).toContainText(title)
    })

    test('individual post shows full content including sections after excerpt', async ({ page }) => {
        await page.goto(`/blog/${slug}`)
        // Both excerpt and post-excerpt content should be visible
        await expect(page.locator('.blog-content')).toContainText('thinking about displaying dir sizes')
        await expect(page.locator('.blog-content')).toContainText('so excited')
        await expect(page.locator('.blog-content')).toContainText('The numbers')
    })

    test('individual post shows date and description', async ({ page }) => {
        await page.goto(`/blog/${slug}`)
        await expect(page.locator('main time')).toBeVisible()
        await expect(page.locator('main header p')).toContainText('scan your drive on startup')
    })

    test('individual post has correct meta tags', async ({ page }) => {
        await page.goto(`/blog/${slug}`)
        const ogImage = page.locator('meta[property="og:image"]')
        await expect(ogImage).toHaveAttribute('content', new RegExp(`/og/${slug}\\.png`))
        const description = page.locator('meta[name="description"]')
        await expect(description).toHaveAttribute('content', /scan your drive on startup/)
        const ogTitle = page.locator('meta[property="og:title"]')
        await expect(ogTitle).toHaveAttribute('content', new RegExp(title.replace(/[.*+?^${}()|[\]\\!]/g, '\\$&')))
    })

    test('individual post has comments section', async ({ page }) => {
        await page.goto(`/blog/${slug}`)
        await expect(page.locator('.comments-section')).toBeVisible()
        await expect(page.getByText('Comments')).toBeVisible()
    })

    test('external links open in new tabs', async ({ page }) => {
        await page.goto(`/blog/${slug}`)
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
        expect(body).toContain(title)
        expect(body).toContain(`/blog/${slug}/`)
    })

    test('OG image returns PNG', async ({ request }) => {
        const response = await request.get(`/og/${slug}.png`)
        expect(response.status()).toBe(200)
        expect(response.headers()['content-type']).toContain('png')
    })

    test('navigation has Blog link', async ({ page }) => {
        await page.goto('/')
        const blogLink = page.getByRole('navigation').getByRole('link', { name: 'Blog' })
        await expect(blogLink).toBeVisible()
    })
})
