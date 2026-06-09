import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'テストスレ',
  res_count: 2,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// Res 2 anchors to res 1 (>>1). The body is server-sanitized, so >> is &gt;&gt;.
function datResponse() {
  return {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025 ID:a', body: '最初のレス' },
      { num: 2, name: '名無し', mail: '', date: '2025 ID:b', body: '&gt;&gt;1 これはアンカー' },
    ],
  }
}

test('clicking a >>N anchor opens the modal without navigating', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )

  await page.goto(THREAD_PATH)
  await expect(page.getByText('これはアンカー')).toBeVisible()

  // The anchor is a span, not a link (no navigation element).
  const anchor = page.locator('.anchor[data-anchor="1"]').first()
  await expect(anchor).toBeVisible()
  expect(await anchor.evaluate((el) => el.tagName)).toBe('SPAN')

  await anchor.click()

  // Modal shows the referenced res; URL stays on the thread path.
  await expect(page.locator('.modal')).toBeVisible()
  await expect(page.locator('.modal').getByText('最初のレス')).toBeVisible()
  await expect(page).toHaveURL(THREAD_PATH)
})
