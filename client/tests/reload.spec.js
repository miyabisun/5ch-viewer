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

// Opening a thread auto-refreshes: it runs the viewer reload (GET, never an
// admin POST) and then renders the freshly-grown dat. There is no manual
// 更新/戻る button anymore.
test('opening a thread auto-refreshes via GET (no POST, no buttons)', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))

  // dat starts at 1 post; the auto reload grows it to 2.
  let count = 1
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(count) }),
  )

  let reloadMethod = null
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
    reloadMethod = route.request().method()
    count = 2
    route.fulfill({
      json: { res_count: 2, read_res: 0, status: 'active', updated: true },
    })
  })

  await page.goto('/')

  // Open the thread: the auto reload (GET) runs, so the latest (2 posts) shows.
  await page.locator('.info').first().click()
  await expect(page.getByText('本文2')).toBeVisible()
  expect(reloadMethod).toBe('GET')

  // The old back/update buttons are gone.
  await expect(page.getByRole('button', { name: /更新/ })).toHaveCount(0)
  await expect(page.getByRole('button', { name: /戻る/ })).toHaveCount(0)
})

// Regression (the "stuck at 111" bug): opening a thread must run the reload (GET)
// and render the grown dat, not the stale stored copy. Here the stored dat starts
// at 111 posts and the reload grows it to 117; the view must show 117.
test('opening a thread shows the grown dat (111 -> 117) after reload', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))

  // Stored dat is 111; the reload pulls the latest (117).
  let count = 111
  let datRequests = 0
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => {
    datRequests += 1
    route.fulfill({ json: datResponse(count) })
  })

  let reloadCalled = false
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
    reloadCalled = true
    count = 117 // the reload grew the dat
    route.fulfill({
      json: { res_count: 117, read_res: 0, status: 'active', updated: true },
    })
  })

  await page.goto('/')
  await page.locator('.info').first().click()

  // The latest post (117) is rendered; the old ceiling (111) is no longer the last.
  await expect(page.getByText('本文117')).toBeVisible()
  expect(reloadCalled).toBe(true)
  // The dat is read after the reload (so the grown copy is what renders).
  expect(datRequests).toBeGreaterThan(0)
})

// The NavBar お気に入り tab is the back path now (always visible, sticky).
test('favorites tab returns from a thread to the list', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(1) }),
  )
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
