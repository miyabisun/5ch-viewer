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

test('update button refreshes the thread via GET (no POST)', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))

  // dat grows from 1 to 2 posts after the reload.
  let count = 1
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(count) }),
  )

  // The viewer's refresh must be a GET (viewer semantics, not an admin POST).
  let reloadMethod = null
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
    reloadMethod = route.request().method()
    count = 2
    route.fulfill({
      json: { res_count: 2, read_res: 0, status: 'active', updated: true },
    })
  })

  await page.goto('/')

  // Open the thread.
  await page.locator('.info').first().click()
  await expect(page.getByText('本文1')).toBeVisible()

  // Click update: triggers the GET reload, then re-loads the dat (now 2 posts).
  await page.getByRole('button', { name: /更新/ }).click()
  await expect(page.getByText('本文2')).toBeVisible()

  expect(reloadMethod).toBe('GET')
})
