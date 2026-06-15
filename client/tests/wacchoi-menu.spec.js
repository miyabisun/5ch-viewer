import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'ワッチョイメニューテストスレ',
  res_count: 3,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// Two posts share the same wacchoi token (7bb6-83IP, appears twice -> badge shown).
// res[0].name must contain the wacchoi token to enable wacchoiEnabled().
function datResponse() {
  return {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00',
        body: '本文1',
        id: null,
      },
      {
        num: 2,
        name: '名無し</b>(ﾜｯﾁｮｲ aaaa-BBBB [::1])<b>',
        mail: '',
        date: '2025/01/01(水) 00:01:00.00',
        body: '本文2',
        id: null,
      },
      {
        num: 3,
        name: 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        mail: '',
        date: '2025/01/01(水) 00:02:00.00',
        body: '本文3',
        id: null,
      },
    ],
  }
}

// Standard route setup shared across tests.
async function setupRoutes(page) {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 3, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ json: [] })
    } else {
      route.fulfill({ json: { ok: true } })
    }
  })
  await page.route(/\/api\/ng-ids\/.*/, (route) =>
    route.fulfill({ json: { ok: true } }),
  )
}

test('right-click on wacchoi badge opens wacchoi-menu modal', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  // Wait for thread to render.
  await expect(page.getByText('本文1')).toBeVisible()

  // The first wacchoi badge (7bb6-83IP, which appears twice -> badge shown).
  const badge = page.locator('.wacchoi-badge').first()
  await expect(badge).toBeVisible()

  // Right-click opens the wacchoi menu.
  await badge.click({ button: 'right' })
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()
})

test('wacchoi-menu modal header shows the wacchoi token', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  const badge = page.locator('.wacchoi-badge').first()
  await badge.click({ button: 'right' })

  // Header should contain the half-width katakana prefix and the token.
  await expect(page.locator('.modal')).toContainText('ﾜｯﾁｮｲ:7bb6-83IP')
})

test('wacchoi-menu contains a copy button', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()

  // The menu must have a コピー button.
  await expect(page.getByRole('button', { name: 'コピー' })).toBeVisible()
})

test('copy writes "ワッチョイ 7bb6-83IP" to clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await page.getByRole('button', { name: 'コピー' }).click()

  const text = await page.evaluate(() => navigator.clipboard.readText())
  expect(text).toBe('ワッチョイ 7bb6-83IP')
})

test('wacchoi-menu closes via × button', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()

  await page.getByRole('button', { name: '閉じる' }).click()
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toHaveCount(0)
})

test('left-click on wacchoi badge opens wacchoi-list (not wacchoi-menu)', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()

  // Left-click -> wacchoi-list modal (regression guard).
  await page.locator('.wacchoi-badge').first().click()
  await expect(page.locator('[data-testid="wacchoi-list"]')).toBeVisible()
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toHaveCount(0)
})
