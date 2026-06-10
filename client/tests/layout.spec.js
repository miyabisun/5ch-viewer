import { test, expect } from '@playwright/test'

// Responsive layout: classic 2ch-style 2 columns on PC (list left + detail
// right, both visible at once), single view on phones. Breakpoint is 768px
// (see docs/discussions.md).

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

function mock(page) {
  page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({
      json: {
        title: FAV.title,
        res_count: 1,
        read_res: 0,
        status: 'active',
        res: [{ num: 1, name: '名無し', mail: '', date: '2025 ID:x', body: '本文1' }],
      },
    }),
  )
  page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 1, read_res: 0, status: 'active' } }),
  )
}

test.describe('PC: two-column layout', () => {
  test.use({ viewport: { width: 1024, height: 800 } })

  test('selecting a thread keeps the list visible and shows the detail beside it', async ({
    page,
  }) => {
    await mock(page)
    await page.goto('/')

    // Detail pane starts with the placeholder; the list is visible.
    await expect(page.locator('.thread .info')).toBeVisible()
    await expect(page.locator('.placeholder')).toBeVisible()

    await page.locator('.thread .info').click()

    // Both panes are visible at once: the list stays on the left, the thread on
    // the right, and the URL is synced.
    await expect(page.locator('.thread .info')).toBeVisible()
    await expect(page.getByText('本文1')).toBeVisible()
    await expect(page).toHaveURL(THREAD_PATH)
    await expect(page.locator('.placeholder')).toHaveCount(0)
  })

  test('direct thread URL opens list + detail together', async ({ page }) => {
    await mock(page)
    await page.goto(THREAD_PATH)

    await expect(page.locator('.thread .info')).toBeVisible()
    await expect(page.getByText('本文1')).toBeVisible()
  })
})

test.describe('phone: single view', () => {
  test.use({ viewport: { width: 390, height: 800 } })

  test('opening a thread hides the list (full-screen detail)', async ({ page }) => {
    await mock(page)
    await page.goto('/')
    await expect(page.locator('.thread .info')).toBeVisible()
    // No desktop placeholder on phones (present in DOM but hidden via CSS).
    await expect(page.locator('.placeholder')).toBeHidden()

    await page.locator('.thread .info').click()
    await expect(page.getByText('本文1')).toBeVisible()
    // The list pane is collapsed in single view (present in DOM but hidden).
    await expect(page.locator('.list-pane')).toBeHidden()
    await expect(page.locator('.thread .info')).toBeHidden()
    await expect(page).toHaveURL(THREAD_PATH)
  })
})
