import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'テストスレ',
  res_count: 1,
  read_res: 0,
  rating: 0,
  status: 'active',
}

function datResponse(count) {
  return {
    title: FAV.title,
    res_count: count,
    read_res: 0,
    status: 'active',
    res: Array.from({ length: count }, (_, i) => ({
      num: i + 1,
      name: '名無し',
      mail: '',
      date: `2025 ID:x${i}`,
      body: `本文${i + 1}`,
    })),
  }
}

// Entry (ChMate model): opening a thread renders the stored dat only and never
// touches 5ch. No reload request fires on open, and the render is not blocked on
// any network round-trip — even a deliberately-delayed reload response cannot
// gate the first paint because reload is not called at all.
test('opening a thread renders stored dat with zero reload requests', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )

  // Stored dat has 1 post; entry must render exactly this, unchanged.
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse(1) }))

  // Reload is registered but must not be hit on entry. If it ever fires, delaying
  // it would expose an entry that blocks on the round-trip — here it should stay
  // at zero. reloadCount lets us assert that.
  let reloadCount = 0
  await page.route(/\/api\/favorites\/.+\/reload$/, async (route) => {
    reloadCount += 1
    await new Promise((r) => setTimeout(r, 1000))
    route.fulfill({
      json: { res_count: 2, read_res: 0, status: 'active', updated: true },
    })
  })

  await page.goto('/')

  // Open the thread: the stored dat (1 post) renders immediately, unblocked.
  await page.locator('.info').first().click()
  await expect(page.getByText('本文1')).toBeVisible()
  expect(reloadCount).toBe(0)

  // The old manual back button is gone from the thread view (detail pane):
  // navigating back is a swipe gesture now. (The footer refresh button is a
  // deliberate replacement for the retired bottom pull-to-refresh gesture, so
  // a 更新 button is expected to exist here.)
  const detailPane = page.locator('.detail-pane')
  await expect(detailPane.getByRole('button', { name: /戻る/ })).toHaveCount(0)
})

// Regression (the "stuck at 111" bug), re-homed onto the footer refresh button:
// entry shows the stored dat (111), and the manual 更新 button runs the reload
// (GET) that grows the dat to 117 and re-renders it. The heal path moved from
// entry to the footer button; the drift-heal coverage (111 -> 117) is preserved.
test('footer refresh grows the drifted dat (111 -> 117) via GET reload', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )

  // Stored dat is 111; the reload pulls the latest (117).
  let count = 111
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => {
    route.fulfill({ json: datResponse(count) })
  })

  let reloadMethod = null
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
    reloadMethod = route.request().method()
    count = 117 // the reload grew the dat
    route.fulfill({
      json: { res_count: 117, read_res: 0, status: 'active', updated: true },
    })
  })

  await page.goto('/')
  await page.locator('.info').first().click()

  // Entry shows the stored 111 (no reload yet); 117 is not present.
  await expect(page.getByText('本文111')).toBeVisible()
  await expect(page.getByText('本文117')).toHaveCount(0)

  // Press the footer 更新 button: reload (GET) fires and the grown dat (117) shows.
  // Scope to the detail pane — the favorites list has its own 更新 button.
  await page.locator('.detail-pane').getByRole('button', { name: '更新' }).click()
  await expect(page.getByText('本文117')).toBeVisible()
  expect(reloadMethod).toBe('GET')
})

// The NavBar お気に入り tab is the back path now (always visible, sticky).
test('favorites tab returns from a thread to the list', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse(1) }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 1, read_res: 0, status: 'active' } }),
  )

  await page.goto('/')
  await page.locator('.info').first().click()
  await expect(page.getByText('本文1')).toBeVisible()

  await page.getByTestId('tab-favorites').click()
  await expect(page).toHaveURL('/')
  await expect(page.locator('.thread .info')).toBeVisible()
  await expect(page.getByText('本文1')).toHaveCount(0)
})
