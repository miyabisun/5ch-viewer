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

// Invariant: visiting the list makes NO 5ch access. Mounting must fire GET /api/favorites
// only — never POST /api/favorites/refresh. The bulk check is the footer button's job.
test('mount fires no refresh; the button triggers exactly one refresh', async ({ page }) => {
  let refreshCount = 0
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => {
    refreshCount++
    route.fulfill({ json: { ok: true, boards: 0 } })
  })
  await page.goto('/')

  // Mount rendered the list, but no refresh was issued.
  await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()
  await page.waitForTimeout(300)
  expect(refreshCount).toBe(0)

  // Pressing the footer button issues exactly one refresh.
  await page.getByTestId('favorites-refresh-btn').click()
  await expect.poll(() => refreshCount).toBe(1)
})

// Invariant: the unread badge reflects only the SQLite res_count (blob-derived). On mount no
// refresh runs, so a thread whose subject grew but whose dat is not yet downloaded shows the
// stored unread count, not the subject count.
test('mount shows unread from stored res_count without fetching the subject', async ({ page }) => {
  let refreshCount = 0
  // Stored res_count=10, read_res=5 -> unread 5. (Subject may say 20 upstream, but until the
  // dat is downloaded res_count stays 10, so the badge must read 5 — never 15.)
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => {
    refreshCount++
    route.fulfill({ json: { ok: true, boards: 0 } })
  })
  await page.goto('/')

  await expect(page.locator('.unread')).toHaveText('5')
  expect(refreshCount).toBe(0)
})

test('refresh button is disabled while refreshing', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  // Make refresh slow so we can observe the disabled state mid-flight.
  // Total time: 200ms (API delay) + 1500ms (REFRESH_RELIST_DELAY_MS) + load() + 1800ms = ~3.5s.
  await mockBase(page, { refreshDelay: 200 })
  await page.goto('/')

  const btn = page.getByTestId('favorites-refresh-btn')
  await expect(btn).toBeVisible()
  // No mount auto-refresh: the button is enabled immediately.
  await expect(btn).toBeEnabled()

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

  // Only the initial mount load has run (no auto-refresh re-list).
  await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()
  await page.waitForTimeout(200)
  const countBefore = listCount

  await page.getByTestId('favorites-refresh-btn').click()

  // After the 1.5 s relist delay + 1.8 s refreshing state buffer, the list
  // should have been re-fetched. Use a generous timeout.
  await expect.poll(() => listCount, { timeout: 5000 }).toBeGreaterThan(countBefore)
})
