import { test, expect } from '@playwright/test'

const FAVS = [
  {
    server: 'egg',
    board: 'applism',
    board_name: 'アプリ',
    thread_id: '1771127001',
    title: 'テストスレ1',
    res_count: 10,
    read_res: 5,
    rating: 3,
    status: 'active',
  },
]

function mockBase(page, { refreshDelay = 0 } = {}) {
  page.route('**/api/favorites/refresh', async (route) => {
    if (refreshDelay > 0) await new Promise((r) => setTimeout(r, refreshDelay))
    route.fulfill({ json: { ok: true, boards: 0 } })
  })
}

// ─── Footer refresh button tests ────────────────────────────────────────────

test('footer refresh button is visible on the favorites list', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await mockBase(page)
  await page.goto('/')

  const btn = page.getByTestId('favorites-refresh-btn')
  await expect(btn).toBeVisible()
  // Icon-only button: no text, just an inline SVG icon.
  await expect(btn.locator('svg')).toBeVisible()
  expect((await btn.textContent()).trim()).toBe('')
})

test('pressing the refresh button calls POST /api/favorites/refresh', async ({ page }) => {
  let refreshCount = 0
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => {
    refreshCount++
    route.fulfill({ json: { ok: true, boards: 0 } })
  })
  await page.goto('/')

  // Wait for the initial auto-refresh (fired on mount) to complete.
  await expect.poll(() => refreshCount).toBeGreaterThanOrEqual(1)
  const countBefore = refreshCount

  await page.getByTestId('favorites-refresh-btn').click()
  await expect.poll(() => refreshCount).toBeGreaterThan(countBefore)
})

test('refresh button is disabled while refreshing', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  // Make refresh slow so we can observe the disabled state mid-flight.
  // Total time: 200ms (API delay) + 1500ms (REFRESH_RELIST_DELAY_MS) + load() + 1800ms = ~3.5s.
  await mockBase(page, { refreshDelay: 200 })
  await page.goto('/')

  const btn = page.getByTestId('favorites-refresh-btn')
  await expect(btn).toBeVisible()

  // Wait for auto-refresh on mount to complete so it doesn't interfere.
  await expect(btn).toBeEnabled({ timeout: 6000 })

  // Click — button should become disabled immediately.
  await btn.click()
  await expect(btn).toBeDisabled()

  // After the full refresh cycle (API + 1.5s relist delay + 1.8s buffer = ~3.5s total),
  // the button re-enables. Use a generous timeout.
  await expect(btn).toBeEnabled({ timeout: 6000 })
})

test('after refresh, GET /api/favorites is called again', async ({ page }) => {
  let listCount = 0
  await page.route('**/api/favorites', (route) => {
    listCount++
    route.fulfill({ json: FAVS })
  })
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.goto('/')

  // Wait for initial load + warm refresh on mount.
  await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()
  await page.waitForTimeout(200)
  const countBefore = listCount

  await page.getByTestId('favorites-refresh-btn').click()

  // After the 1.5 s relist delay + 1.8 s refreshing state buffer, the list
  // should have been re-fetched. Use a generous timeout.
  await expect.poll(() => listCount, { timeout: 5000 }).toBeGreaterThan(countBefore)
})
