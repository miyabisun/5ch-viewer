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

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// Returns a dat payload with `count` reses.
function datResponse(count = 40) {
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

// Registers all routes needed for the ThreadView to mount and load.
function mockRoutes(page, opts = {}) {
  const { datCount = 40, reloadHandler } = opts
  page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(datCount) }),
  )
  if (reloadHandler) {
    page.route(/\/api\/favorites\/.+\/reload$/, reloadHandler)
  } else {
    page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({
        json: { res_count: datCount, read_res: 0, status: 'active' },
      }),
    )
  }
}

// ─── Footer refresh button ──────────────────────────────────────────────────

test.describe('footer refresh button', () => {
  test.use({ viewport: { width: 390, height: 700 } })

  test('write button is at the left end and refresh at the right end', async ({ page }) => {
    await mockRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    const write = await page.getByRole('button', { name: '書き込む' }).boundingBox()
    const refresh = await page.getByRole('button', { name: '更新' }).boundingBox()
    expect(write.x).toBeLessThan(refresh.x)
  })

  test('refresh button click fires a GET reload and marks only new reses unread', async ({
    page,
  }) => {
    const INITIAL_COUNT = 40
    const ADDED_COUNT = 2

    let reloadCallCount = 0
    page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    // First dat call returns initial dat; after reload it returns the grown dat.
    let datCallCount = 0
    page.route(/\/api\/favorites\/.+\/dat$/, (route) => {
      datCallCount++
      route.fulfill({
        json:
          datCallCount === 1
            ? datResponse(INITIAL_COUNT)
            : datResponse(INITIAL_COUNT + ADDED_COUNT),
      })
    })
    page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
      reloadCallCount++
      route.fulfill({
        json: {
          res_count: INITIAL_COUNT + ADDED_COUNT,
          read_res: 0,
          status: 'active',
        },
      })
    })
    page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { read_res: 0 } })
      } else {
        route.fulfill({ json: { ok: true } })
      }
    })

    await page.goto(THREAD_PATH)
    await expect(page.getByText(`本文${INITIAL_COUNT}`, { exact: true })).toBeVisible()

    const reloadsBefore = reloadCallCount

    // Press the footer refresh button.
    await page.getByRole('button', { name: '更新' }).click()

    // Wait for the refresh to complete (new reses appear).
    await expect(
      page.getByText(`本文${INITIAL_COUNT + ADDED_COUNT}`, { exact: true }),
    ).toBeVisible()
    expect(reloadCallCount).toBe(reloadsBefore + 1)

    // Old reses (num 1..INITIAL_COUNT) must NOT have the .unread class.
    for (let i = 1; i <= INITIAL_COUNT; i++) {
      await expect(page.locator(`.res[data-res="${i}"]`)).not.toHaveClass(/unread/)
    }
    // New reses (num INITIAL_COUNT+1..) MUST have .unread.
    for (let i = INITIAL_COUNT + 1; i <= INITIAL_COUNT + ADDED_COUNT; i++) {
      await expect(page.locator(`.res[data-res="${i}"]`)).toHaveClass(/unread/)
    }
  })

  test('no double-fire: clicking refresh again while in flight is ignored', async ({ page }) => {
    let reloadCount = 0
    // Make reload take 800 ms so a second click lands while the first is still in flight.
    await mockRoutes(page, {
      reloadHandler: async (route) => {
        reloadCount++
        await new Promise((r) => setTimeout(r, 800))
        route.fulfill({
          json: { res_count: 40, read_res: 0, status: 'active' },
        })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    const countBefore = reloadCount
    const btn = page.getByRole('button', { name: '更新' })

    // First click starts the refresh; button becomes disabled while in flight.
    await btn.click()
    await expect(btn).toBeDisabled()

    // A forced second click while disabled must not start a second reload.
    await btn.click({ force: true })

    // Wait for the in-flight reload to settle.
    await page.waitForTimeout(1000)
    expect(reloadCount - countBefore).toBe(1)
  })

  test('opening a thread without scrolling shows unread bar on unread reses (regression)', async ({
    page,
  }) => {
    // FAV has read_res=0, so all reses start as unread.
    const COUNT = 5
    page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse(COUNT) }))
    page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({
        json: { res_count: COUNT, read_res: 0, status: 'active' },
      }),
    )
    page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { read_res: 0 } })
      } else {
        route.fulfill({ json: { ok: true } })
      }
    })

    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    // All reses must have .unread since read_res=0.
    for (let i = 1; i <= COUNT; i++) {
      await expect(page.locator(`.res[data-res="${i}"]`)).toHaveClass(/unread/)
    }
  })
})
