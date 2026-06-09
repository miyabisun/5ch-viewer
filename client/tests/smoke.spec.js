import { test, expect } from '@playwright/test'

test('top page loads (favorites empty)', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.goto('/')
  await expect(page).toHaveTitle(/goch-viewer/)
})
