import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'ワッチョイテストスレ',
  res_count: 4,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// res1, res2: same wacchoi (7bb6-83IP, total=2)
// res3: unique wacchoi (aaaa-1111, total=1)
// res4: no wacchoi
function datResponse() {
  return {
    title: FAV.title,
    res_count: 4,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>',
        mail: '',
        date: '2026/06/15(月) 00:00:01.00 ID:AAA11111',
        body: '1つ目のレス',
      },
      {
        num: 2,
        name: 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>',
        mail: '',
        date: '2026/06/15(月) 00:00:02.00 ID:AAA11111',
        body: '2つ目のレス（同じワッチョイ）',
      },
      {
        num: 3,
        name: 'foo </b>(ﾜｯﾁｮｲ aaaa-1111 [::1])<b>',
        mail: '',
        date: '2026/06/15(月) 00:00:03.00 ID:BBB22222',
        body: '3つ目のレス（単独ワッチョイ）',
      },
      {
        num: 4,
        name: '名無しさん',
        mail: '',
        date: '2026/06/15(月) 00:00:04.00 ID:CCC33333',
        body: '4つ目のレス（ワッチョイ無し）',
      },
    ],
  }
}

async function setupRoutes(page, dat) {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: dat ?? datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 4, read_res: 0, status: 'active' } }),
  )
}

// ---------------------------------------------------------------------------
// Name text no longer contains raw wacchoi token
// ---------------------------------------------------------------------------
test('name span does not contain the raw wacchoi parenthesised group', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('1つ目のレス')).toBeVisible()

  // The .name span for res1 must not contain the wacchoi text
  const nameSpan = page.locator('.res').nth(0).locator('.name')
  const nameText = await nameSpan.textContent()
  expect(nameText).not.toContain('ﾜｯﾁｮｲ')
  expect(nameText).not.toContain('7bb6-83IP')
  expect(nameText).not.toContain('[2400:')
  // The base name is preserved
  expect(nameText).toContain('iPhone774G')
})

// ---------------------------------------------------------------------------
// Wacchoi badge is shown for total>=2 (coloured) and total=1 (muted)
// ---------------------------------------------------------------------------
test('wacchoi badge appears for res1 (total=2) with order/total counts', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('1つ目のレス')).toBeVisible()

  // res1 is the 1st of 2 posts with wacchoi 7bb6-83IP
  const badge = page.locator('.res').nth(0).locator('[data-wacchoi="7bb6-83IP"]')
  await expect(badge).toBeVisible()
  await expect(badge).toContainText('ﾜｯﾁｮｲ:7bb6-83IP')
  await expect(badge).toContainText('(1/2)')
})

test('wacchoi badge appears for res2 (total=2) with order/total counts', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('2つ目のレス（同じワッチョイ）')).toBeVisible()

  // res2 is the 2nd of 2 posts with wacchoi 7bb6-83IP
  const badge = page.locator('.res').nth(1).locator('[data-wacchoi="7bb6-83IP"]')
  await expect(badge).toBeVisible()
  await expect(badge).toContainText('ﾜｯﾁｮｲ:7bb6-83IP')
  await expect(badge).toContainText('(2/2)')
})

test('wacchoi badge appears for res3 (total=1) without order/total counts', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('3つ目のレス（単独ワッチョイ）')).toBeVisible()

  // res3 has a unique wacchoi (total=1) — badge shown but no "(1/1)" suffix
  const badge = page.locator('.res').nth(2).locator('[data-wacchoi="aaaa-1111"]')
  await expect(badge).toBeVisible()
  await expect(badge).toContainText('ﾜｯﾁｮｲ:aaaa-1111')
  const text = await badge.textContent()
  expect(text).not.toContain('/')
})

test('no wacchoi badge for res4 (no wacchoi in name)', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('4つ目のレス（ワッチョイ無し）')).toBeVisible()

  const badge = page.locator('.res').nth(3).locator('[data-wacchoi]')
  await expect(badge).toHaveCount(0)
})

// ---------------------------------------------------------------------------
// Wacchoi name for res4 (no wacchoi) is displayed unchanged
// ---------------------------------------------------------------------------
test('res4 with no wacchoi shows plain name unchanged', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('4つ目のレス（ワッチョイ無し）')).toBeVisible()

  const nameSpan = page.locator('.res').nth(3).locator('.name')
  await expect(nameSpan).toHaveText('名無しさん')
})

// ---------------------------------------------------------------------------
// Clicking the wacchoi badge opens the wacchoi list modal
// ---------------------------------------------------------------------------
test('clicking wacchoi badge opens the wacchoi list modal', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('1つ目のレス')).toBeVisible()

  const badge = page.locator('.res').nth(0).locator('[data-wacchoi="7bb6-83IP"]')
  await badge.click()

  // A modal must appear containing the wacchoi-filtered posts
  await expect(page.locator('.modal')).toBeVisible()
})
