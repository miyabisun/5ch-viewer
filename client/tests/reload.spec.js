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

// The NavBar お気に入り tab is the back path now (always visible, sticky).
test('favorites tab returns from a thread to the list', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
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
