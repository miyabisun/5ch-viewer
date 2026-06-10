import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '固定タイトルのテストスレ',
  res_count: 60,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`
const COUNT = 60

function datResponse() {
  return {
    title: FAV.title,
    res_count: COUNT,
    read_res: 0,
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

test('the thread title stays pinned at the top while scrolling', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: COUNT, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  const title = page.getByTestId('thread-title')
  await expect(title).toHaveText(FAV.title)
  await expect(title).toBeVisible()

  // Scroll well down the thread.
  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(0)

  // The title is still visible (sticky) and pinned near the top of the viewport.
  await expect(title).toBeInViewport()
  const top = await title.evaluate((el) => el.getBoundingClientRect().top)
  expect(top).toBeLessThan(120)
})
