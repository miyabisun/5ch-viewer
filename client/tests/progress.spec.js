import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'テストスレ',
  res_count: 30,
  read_res: 28,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`
const COUNT = 30

// dat with COUNT posts; the saved read position (read_res) is near the bottom.
function datResponse() {
  return {
    title: FAV.title,
    res_count: COUNT,
    read_res: FAV.read_res,
    status: 'active',
    res: Array.from({ length: COUNT }, (_, i) => ({
      num: i + 1,
      name: '名無し',
      mail: '',
      date: `2025 ID:x${i}`,
      body: `本文${i + 1}`,
    })),
  }
}

test('opening a thread restores the saved read position (auto-scroll)', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  // Opening auto-refreshes (GET reload); mock it so the open path is deterministic.
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: COUNT, read_res: FAV.read_res, status: 'active' } }),
  )
  // progress GET/POST must not 405; respond to both so the page logic completes.
  await page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ json: { read_res: FAV.read_res } })
    } else {
      route.fulfill({ json: { ok: true } })
    }
  })

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1', { exact: true })).toBeVisible()

  // The page auto-scrolls to the saved read position (res 28), so it is visible
  // and the page is scrolled away from the top.
  const target = page.locator('.res[data-res="28"]')
  await expect(target).toBeInViewport()
  expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0)
})
