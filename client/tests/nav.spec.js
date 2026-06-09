import { test, expect } from '@playwright/test'

test('nav tabs switch between favorites and register pages', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.goto('/')

  // Default: favorites page. The register form (mode select) is not shown.
  await expect(page.getByTestId('tab-favorites')).toHaveClass(/active/)
  await expect(page.locator('.add')).toHaveCount(0)

  // Switch to register page: the search/url form appears.
  await page.getByTestId('tab-register').click()
  await expect(page.getByTestId('tab-register')).toHaveClass(/active/)
  await expect(page.locator('.add')).toBeVisible()
  await expect(page.getByPlaceholder('キーワードで検索')).toBeVisible()

  // Switch back to favorites: the form disappears.
  await page.getByTestId('tab-favorites').click()
  await expect(page.getByTestId('tab-favorites')).toHaveClass(/active/)
  await expect(page.locator('.add')).toHaveCount(0)
})
