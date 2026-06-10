import { test, expect } from '@playwright/test'

test('theme toggle flips data-theme, persists, and survives reload', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  // Force a known OS default (light) so the toggle outcome is deterministic.
  await page.emulateMedia({ colorScheme: 'light' })
  await page.goto('/')

  const html = page.locator('html')
  await expect(html).toHaveAttribute('data-theme', 'light')

  // Toggle to dark: attribute changes and is saved to localStorage.
  await page.getByTestId('theme-toggle').click()
  await expect(html).toHaveAttribute('data-theme', 'dark')
  expect(await page.evaluate(() => localStorage.getItem('goch-theme'))).toBe('dark')

  // Reload: stored override is restored (independent of OS preference).
  await page.reload()
  await expect(html).toHaveAttribute('data-theme', 'dark')

  // Toggle back to light persists too.
  await page.getByTestId('theme-toggle').click()
  await expect(html).toHaveAttribute('data-theme', 'light')
  expect(await page.evaluate(() => localStorage.getItem('goch-theme'))).toBe('light')
})

test('theme follows OS preference when no override stored', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.goto('/')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
})
