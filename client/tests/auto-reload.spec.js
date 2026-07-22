import { test, expect } from '@playwright/test'

const FAVORITE = {
  server: 'egg',
  board: 'software',
  board_name: 'ソフトウェア',
  thread_id: '1000000001',
  title: '更新前',
  res_count: 10,
  read_res: 10,
  rating: 0,
  status: 'active',
}

test('visible favorites page reloads cached list every 60 seconds only', async ({ page }) => {
  await page.clock.install()

  let listRequests = 0
  const upstreamRequests = []
  page.on('request', (request) => {
    if (/\/api\/favorites\/(refresh|.+\/(reload|dat))$/.test(request.url())) {
      upstreamRequests.push(request.url())
    }
  })
  await page.route('**/api/favorites', (route) => {
    listRequests += 1
    route.fulfill({
      json: [{ ...FAVORITE, title: listRequests === 1 ? '更新前' : '更新後' }],
    })
  })

  await page.goto('/')
  await expect(page.getByText('更新前')).toBeVisible()
  expect(listRequests).toBe(1)

  await page.clock.fastForward(59_999)
  expect(listRequests).toBe(1)

  await page.clock.fastForward(1)
  await expect(page.getByText('更新後')).toBeVisible()
  expect(listRequests).toBe(2)
  expect(upstreamRequests).toEqual([])
})
