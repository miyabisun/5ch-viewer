import { test, expect } from '@playwright/test'

test('top page loads (favorites empty)', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.goto('/')
  await expect(page).toHaveTitle(/viewer-of-5ch/)
})
