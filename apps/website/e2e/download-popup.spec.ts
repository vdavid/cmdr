import { test, expect, type Page } from '@playwright/test'

/** Click the hero download button without starting a real download: the popup listens on the
 * document, so cancelling the anchor's default action still lets the click reach it. */
async function clickDownload(page: Page) {
  const link = page.locator('[data-download-link]').first()
  await link.evaluate((el) => el.addEventListener('click', (e) => e.preventDefault()))
  await link.click()
}

test.describe('Download popup', () => {
  test('asks for a GitHub star and a release email after the first download click', async ({ page }) => {
    await page.goto('/')
    await clickDownload(page)

    const dialog = page.getByRole('dialog', { name: 'Your download has started' })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByRole('link', { name: /Star Cmdr on GitHub/ })).toHaveAttribute(
      'href',
      'https://github.com/vdavid/cmdr',
    )
    await expect(dialog.locator('input[type="email"]')).toBeVisible()
    await expect(dialog.getByRole('link', { name: 'Discord' })).toBeVisible()
  })

  test('thanks subscribers instead of asking for their email again', async ({ page }) => {
    await page.goto('/')
    await page.evaluate(() => localStorage.setItem('newsletter-subscribed', 'true'))
    await page.reload()
    await clickDownload(page)

    const dialog = page.getByRole('dialog', { name: 'Your download has started' })
    await expect(dialog).toBeVisible()
    await expect(dialog.locator('input[type="email"]')).toBeHidden()
    await expect(dialog.getByText(/already on the release email list/)).toBeVisible()
  })

  test('shows only once per visitor', async ({ page }) => {
    await page.goto('/')
    await clickDownload(page)
    const dialog = page.getByRole('dialog', { name: 'Your download has started' })
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: 'Close' }).first().click()
    await expect(dialog).toBeHidden()

    await clickDownload(page)
    await expect(dialog).toBeHidden()
  })
})
