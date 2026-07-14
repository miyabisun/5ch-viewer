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

test.describe('phone: saved position beyond the available dat', () => {
  test.use({ viewport: { width: 390, height: 700 } })

  test('clamps the boundary to the newest post and aligns it to the viewport bottom', async ({
    page,
  }) => {
    // Simulate stale progress beyond the available dat: read_res=50 with only
    // 30 posts. The newest available post becomes the safe boundary.
    const phoneFav = { ...FAV, read_res: 50 }

    await page.route('**/api/favorites', (route) => route.fulfill({ json: [phoneFav] }))
    await page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
      route.fulfill({
        json: {
          title: phoneFav.title,
          res_count: COUNT,
          read_res: 50,
          status: 'active',
          res: Array.from({ length: COUNT }, (_, i) => ({
            num: i + 1,
            name: '名無し',
            mail: '',
            date: `2025 ID:x${i}`,
            body: `本文${i + 1}`,
          })),
        },
      }),
    )
    await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({ json: { res_count: COUNT, read_res: 50, status: 'active' } }),
    )
    await page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { read_res: 50 } })
      } else {
        route.fulfill({ json: { ok: true } })
      }
    })

    await page.goto(THREAD_PATH)
    await expect(page.locator('.thread-body > .res').first()).toHaveAttribute('data-res', '30')
    await expect(page.getByTestId('read-boundary')).toHaveText('前回ここまで')
    await expect
      .poll(() =>
        page.locator('.thread-body').evaluate((body) => {
          const marker = body.querySelector('[data-testid="read-boundary"]')
          return Math.abs(
            body.getBoundingClientRect().bottom - marker.getBoundingClientRect().bottom,
          )
        }),
      )
      .toBeLessThanOrEqual(1)
  })
})

test.describe('PC: unread badge decreases as user scrolls (onprogress)', () => {
  test.use({ viewport: { width: 1024, height: 800 } })

  test('scrolling to lower reses immediately reduces the unread badge in the list pane', async ({
    page,
  }) => {
    // FAV has res_count=30, read_res=28 → unread badge = 2 initially.
    await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    await page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
      route.fulfill({ json: datResponse() }),
    )
    await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({ json: { res_count: COUNT, read_res: FAV.read_res, status: 'active' } }),
    )
    await page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { read_res: FAV.read_res } })
      } else {
        route.fulfill({ json: { ok: true } })
      }
    })

    await page.goto('/')
    // Wait for the list to render and click the thread to open it.
    await expect(page.getByText(FAV.title)).toBeVisible()
    // Initially unread badge = 30 - 28 = 2 (ThreadRow badge in list pane).
    await expect(page.locator('.list-pane .unread')).toHaveText('2')

    // Open the thread in the right pane.
    await page.getByText(FAV.title).click()
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    // Scroll the last res (30) into view inside the detail-pane to trigger IntersectionObserver.
    await page.locator('.res[data-res="30"]').scrollIntoViewIfNeeded()

    // The unread badge (ThreadRow .unread span, inside .list-pane) should drop to 0.
    // Use .list-pane .unread to avoid matching .res.unread elements in the detail pane.
    await expect(page.locator('.list-pane .unread')).toHaveCount(0)
  })
})

test('opening a thread puts the previous-read marker at the viewport bottom', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  // Entry renders stored dat only (no reload); defensive mock, never fires on open.
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
  const posts = page.locator('.thread-body > .res')
  await expect(posts.first()).toHaveAttribute('data-res', '30')
  await expect(posts.nth(1)).toHaveAttribute('data-res', '29')
  await expect(posts.nth(2)).toHaveAttribute('data-res', '28')
  await expect(page.getByTestId('read-boundary')).toHaveText('前回ここまで')
  await expect
    .poll(() =>
      page.locator('.thread-body').evaluate((body) => {
        const marker = body.querySelector('[data-testid="read-boundary"]')
        return Math.abs(
          body.getBoundingClientRect().bottom - marker.getBoundingClientRect().bottom,
        )
      }),
    )
    .toBeLessThanOrEqual(1)
})
