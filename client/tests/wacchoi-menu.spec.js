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
async function setupRoutes(page, { ngWacchoi = [] } = {}) {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
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
  await page.route(/\/api\/ng-words(\?|$)/, (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route('**/api/ng-wacchoi', (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ json: ngWacchoi })
    } else {
      route.fulfill({ json: { ok: true } })
    }
  })
  await page.route(/\/api\/boards\/.+\/wacchoi-search/, (route) => route.fulfill({ json: [] }))
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
  await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)
})

test('long-press on wacchoi badge opens only the wacchoi menu', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  const badge = page.locator('.wacchoi-badge').first()
  await badge.dispatchEvent('pointerdown', {
    pointerType: 'touch',
    pointerId: 1,
  })
  await page.waitForTimeout(550)
  await badge.dispatchEvent('pointerup', {
    pointerType: 'touch',
    pointerId: 1,
  })

  await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()
  await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)
})

test('wacchoi tap still works after closing a reply menu opened from the res header', async ({
  page,
}) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  const num = page.locator('.res .num').first()
  await num.dispatchEvent('pointerdown', {
    pointerType: 'touch',
    pointerId: 1,
  })
  await page.waitForTimeout(550)
  await num.dispatchEvent('pointerup', { pointerType: 'touch', pointerId: 1 })
  await num.dispatchEvent('click')
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
  await page.getByRole('button', { name: '閉じる' }).click()

  await page.locator('.wacchoi-badge').first().click()
  await expect(page.locator('[data-testid="wacchoi-list"]')).toBeVisible()
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

test('wacchoi-menu contains NG button, copy button, and search button', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()

  // The menu must have all three action buttons.
  await expect(page.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'コピー' })).toBeVisible()
  await expect(page.getByRole('button', { name: '取得済みスレから検索' })).toBeVisible()
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

// --- New tests: NG wacchoi button and API calls ---

test('NGﾜｯﾁｮｲに追加 calls POST /api/ng-wacchoi with correct suffix, board, week_key', async ({
  page,
}) => {
  const addRequests = []
  await setupRoutes(page)
  // Override ng-wacchoi to capture POST.
  await page.route(/\/api\/ng-wacchoi(?:\?.*)?$/, async (route) => {
    if (route.request().method() === 'POST') {
      addRequests.push(route.request().postDataJSON())
      route.fulfill({ json: { ok: true } })
    } else {
      route.fulfill({ json: [] })
    }
  })

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()
  await page.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }).click()

  await expect.poll(() => addRequests.length).toBe(1)
  const req = addRequests[0]
  // suffix must be the last 4 chars of the token (after the hyphen)
  expect(req.suffix).toBe('83IP')
  // board must match the fav board
  expect(req.board).toBe('applism')
  // week_key is the Sunday that began the week of 2025/01/01(水): 2024/12/29.
  expect(req.week_key).toBe('2024/12/29')
  // full wacchoi token
  expect(req.wacchoi).toBe('7bb6-83IP')
})

test('NGﾜｯﾁｮｲ追加後はワッチョイメニューに削除ボタンを出さない', async ({ page }) => {
  // week_key for 2025/01/01(水) -> Sunday 2024/12/29
  const existingNg = [
    {
      suffix: '83IP',
      board: 'applism',
      week_key: '2024/12/29',
      wacchoi: '7bb6-83IP',
      created_at: 0,
    },
  ]

  const getRequests = []
  await setupRoutes(page)
  await page.route('**/api/ng-wacchoi', async (route) => {
    if (route.request().method() === 'GET') {
      getRequests.push(1)
      // Return the NG entry after the first GET (simulating state after add).
      route.fulfill({ json: getRequests.length > 1 ? existingNg : [] })
    } else {
      route.fulfill({ json: { ok: true } })
    }
  })

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  // Open menu and add NG.
  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await page.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }).click()

  // The original badge is replaced by the NG disclosure, so its old removal
  // action is physically unavailable and must not remain in the DOM.
  await expect(page.locator('del.ng').filter({ hasText: '1 NGワッチョイ' })).toHaveText(
    '1 NGワッチョイ',
  )
  await expect(page.locator('.wacchoi-badge[data-wacchoi="7bb6-83IP"]')).toHaveCount(0)
  await expect(page.getByRole('button', { name: 'NGﾜｯﾁｮｲから削除' })).toHaveCount(0)
})

test('NG wacchoi post menu removes the matching scoped entry', async ({ page }) => {
  const existingNg = [
    {
      suffix: '83IP',
      board: 'applism',
      week_key: '2024/12/29',
      wacchoi: '7bb6-83IP',
      created_at: 0,
    },
  ]
  const deleteRequests = []
  await setupRoutes(page, { ngWacchoi: existingNg })
  await page.route(/\/api\/ng-wacchoi(?:\?.*)?$/, async (route) => {
    if (route.request().method() === 'DELETE') {
      const url = new URL(route.request().url())
      deleteRequests.push(Object.fromEntries(url.searchParams))
      await route.fulfill({ json: { ok: true } })
    } else {
      await route.fulfill({ json: existingNg })
    }
  })

  await page.goto(THREAD_PATH)
  const ngHeader = page.locator('del.ng').filter({ hasText: '1 NGワッチョイ' })
  await expect(ngHeader).toHaveText('1 NGワッチョイ')
  await ngHeader.click({ button: 'right' })
  await expect(page.locator('[data-testid="ng-menu"]')).toBeVisible()
  await page.getByRole('button', { name: 'NGワッチョイから削除' }).click()

  await expect.poll(() => deleteRequests.length).toBe(1)
  expect(deleteRequests[0]).toEqual({
    suffix: '83IP',
    board: 'applism',
    week_key: '2024/12/29',
  })
})

// --- NG scope regression tests ---

// dat with posts from two different prefixes (IP differs) but same suffix.
// Also includes posts on both sides of the Sunday 00:00 change and one with a
// different suffix.
function datForNgScope() {
  return {
    title: FAV.title,
    res_count: 5,
    read_res: 0,
    status: 'active',
    res: [
      {
        // Same suffix, same week (2025/01/01 is Wed -> the week began Sun 2024/12/29).
        num: 1,
        name: 'A</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00',
        body: '同板同週同末尾A',
        id: null,
      },
      {
        // Different prefix (IP differs), same suffix, same week -> must also be NG.
        num: 2,
        name: 'B</b>(ﾜｯﾁｮｲ cccc-83IP [::3])<b>',
        mail: '',
        date: '2025/01/01(水) 00:01:00.00',
        body: '同板同週同末尾B(前半違い)',
        id: null,
      },
      {
        // Same suffix, last second of the same week (Saturday) -> must also be NG.
        num: 3,
        name: 'C</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        mail: '',
        date: '2025/01/04(土) 23:59:59.99',
        body: '同週土曜同末尾',
        id: null,
      },
      {
        // Same suffix, first second of the next week (Sunday 00:00) -> another poster.
        num: 4,
        name: 'D</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        mail: '',
        date: '2025/01/05(日) 00:00:00.00',
        body: '翌週日曜同末尾',
        id: null,
      },
      {
        // Different suffix, same week.
        num: 5,
        name: 'E</b>(ﾜｯﾁｮｲ 7bb6-ZZZZ [2400::])<b>',
        mail: '',
        date: '2025/01/01(水) 00:02:00.00',
        body: '同板同週別末尾',
        id: null,
      },
    ],
  }
}

test('NG scope: same suffix + same board + same week are hidden (different prefix is also NG)', async ({
  page,
}) => {
  // Register suffix 83IP for board applism, week_key 2024/12/29 (week of 2025/01/01 Wed).
  const ngWacchoi = [
    {
      suffix: '83IP',
      board: 'applism',
      week_key: '2024/12/29',
      wacchoi: '7bb6-83IP',
      created_at: 0,
    },
  ]

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datForNgScope() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 5, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-words(\?|$)/, (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route('**/api/ng-wacchoi', (route) => {
    if (route.request().method() === 'GET') route.fulfill({ json: ngWacchoi })
    else route.fulfill({ json: { ok: true } })
  })
  await page.route(/\/api\/boards\/.+\/wacchoi-search/, (route) => route.fulfill({ json: [] }))

  await page.goto(THREAD_PATH)

  // res1 (7bb6-83IP, week 2024/12/29) must be NG-hidden.
  await expect(page.getByText('同板同週同末尾A')).toHaveCount(0)
  // res2 (cccc-83IP, different prefix but same suffix + week) must ALSO be NG-hidden.
  await expect(page.getByText('同板同週同末尾B(前半違い)')).toHaveCount(0)
  // res3 is Saturday of the same week: the wacchoi has not changed yet -> NG-hidden.
  await expect(page.getByText('同週土曜同末尾')).toHaveCount(0)
  // The three NG res headers show as del.ng.
  await expect(page.locator('del.ng')).toHaveCount(3)
})

test('NG scope: different week with same suffix is NOT hidden (誤爆しない)', async ({ page }) => {
  const ngWacchoi = [
    {
      suffix: '83IP',
      board: 'applism',
      week_key: '2024/12/29',
      wacchoi: '7bb6-83IP',
      created_at: 0,
    },
  ]

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datForNgScope() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 5, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-words(\?|$)/, (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route('**/api/ng-wacchoi', (route) => {
    if (route.request().method() === 'GET') route.fulfill({ json: ngWacchoi })
    else route.fulfill({ json: { ok: true } })
  })
  await page.route(/\/api\/boards\/.+\/wacchoi-search/, (route) => route.fulfill({ json: [] }))

  await page.goto(THREAD_PATH)

  // res4 is 2025/01/05(日) 00:00 -> week_key 2025/01/05 (the wacchoi changed) -> NOT NG.
  await expect(page.getByText('翌週日曜同末尾')).toBeVisible()
  // res5 has different suffix ZZZZ -> NOT NG.
  await expect(page.getByText('同板同週別末尾')).toBeVisible()
})

// --- Wacchoi search modal test ---

test('取得済みスレから検索 shows wacchoi-search-result modal', async ({ page }) => {
  const searchResult = [
    {
      thread_id: '9999999999',
      title: '別スレ',
      res: [
        {
          num: 5,
          name: 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
          mail: '',
          date: '2025/01/01',
          body: '検索ヒット',
          id: null,
        },
      ],
    },
  ]
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 3, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-words(\?|$)/, (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route('**/api/ng-wacchoi', (route) => {
    if (route.request().method() === 'GET') route.fulfill({ json: [] })
    else route.fulfill({ json: { ok: true } })
  })
  await page.route(/\/api\/boards\/.+\/wacchoi-search/, (route) =>
    route.fulfill({ json: searchResult }),
  )

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await page.getByRole('button', { name: '取得済みスレから検索' }).click()

  const resultModal = page.locator('[data-testid="wacchoi-search-result"]')
  await expect(resultModal).toBeVisible()
  await expect(page.getByText('検索ヒット')).toBeVisible()
})

test('取得済みスレから検索 shows 該当なし when no results', async ({ page }) => {
  await setupRoutes(page)

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await page.getByRole('button', { name: '取得済みスレから検索' }).click()

  await expect(page.locator('[data-testid="wacchoi-search-result"]')).toBeVisible()
  await expect(page.getByText('該当なし')).toBeVisible()
})

// --- week_key boundary test: Sunday 00:00 JST ---

// Adds the first badge to the NG list and returns the POST body, for a thread whose
// two reses were posted at `date`.
async function addNgAt(page, date) {
  const addRequests = []
  await setupRoutes(page)
  const dat = {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [1, 2].map((num) => ({
      num,
      name: 'A</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
      mail: '',
      date,
      body: `境界レス${num}`,
      id: null,
    })),
  }
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: dat }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-wacchoi', async (route) => {
    if (route.request().method() === 'POST') {
      addRequests.push(route.request().postDataJSON())
      route.fulfill({ json: { ok: true } })
    } else {
      route.fulfill({ json: [] })
    }
  })

  await page.goto(THREAD_PATH)
  await expect(page.getByText('境界レス1')).toBeVisible()
  await page.locator('.wacchoi-badge').first().click({ button: 'right' })
  await page.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }).click()
  await expect.poll(() => addRequests.length).toBe(1)
  return addRequests[0]
}

test('week_key: 2025/01/05(日) 00:00 starts a new week (week_key 2025/01/05)', async ({ page }) => {
  const req = await addNgAt(page, '2025/01/05(日) 00:00:00.00')
  expect(req.week_key).toBe('2025/01/05')
})

test('week_key: 2025/01/04(土) 23:59 belongs to the week of 2024/12/29', async ({ page }) => {
  const req = await addNgAt(page, '2025/01/04(土) 23:59:59.99')
  expect(req.week_key).toBe('2024/12/29')
})

// --- Grouping by suffix ---

// One poster seen through three lines (prefix differs, suffix 83IP), one unrelated
// poster, and one poster whose suffix contains '/'.
function datForSuffixGroup() {
  const res = [
    ['iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>', '自宅回線'],
    ['iPhone774G </b>(ﾜｯﾁｮｲ cccc-83IP [::3])<b>', '別回線'],
    ['iPhone774G </b>(ｱｳｱｳｳｰ Sa1b-83IP)<b>', 'モバイル回線'],
    ['名無し</b>(ﾜｯﾁｮｲ 7bb6-ZZZZ [2400::])<b>', '別人'],
    ['名無し</b>(ﾜｯﾁｮｲ 86fb-/dMI)<b>', 'スラッシュA'],
    ['名無し</b>(ﾜｯﾁｮｲ 86b0-/dMI)<b>', 'スラッシュB'],
  ]
  return {
    title: FAV.title,
    res_count: res.length,
    read_res: 0,
    status: 'active',
    res: res.map(([name, body], i) => ({
      num: i + 1,
      name,
      mail: '',
      date: `2025/01/01(水) 00:0${i}:00.00`,
      body,
      id: null,
    })),
  }
}

async function gotoSuffixGroup(page, opts) {
  await setupRoutes(page, opts)
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datForSuffixGroup() }),
  )
  await page.goto(THREAD_PATH)
  await expect(page.getByText('自宅回線')).toBeVisible()
}

test('tapping a badge lists every res with the same suffix, whatever the prefix', async ({
  page,
}) => {
  await gotoSuffixGroup(page)

  // Tap the badge of the second line (cccc-83IP): the list is keyed by 83IP.
  await page.locator('.wacchoi-badge[data-wacchoi="cccc-83IP"]').click()
  const list = page.locator('[data-testid="wacchoi-list"]')
  await expect(list).toBeVisible()
  await expect(page.locator('.modal')).toContainText('ﾜｯﾁｮｲ末尾:83IP（3件）')
  await expect(list.locator('.res')).toHaveCount(3)
  await expect(list).toContainText('自宅回線')
  await expect(list).toContainText('別回線')
  await expect(list).toContainText('モバイル回線')
  // Same prefix as res 1 but another suffix: not the same poster.
  await expect(list).not.toContainText('別人')
})

test('reses sharing a suffix take the same heat colour; a lone suffix takes none', async ({
  page,
}) => {
  await gotoSuffixGroup(page)

  const nameOf = (token) =>
    page.locator('.name', {
      has: page.locator(`.wacchoi-badge[data-wacchoi="${token}"]`),
    })
  // Three reses share 83IP -> level 2 on each of them, although every full token is unique.
  await expect(nameOf('7bb6-83IP')).toHaveClass(/id-l2/)
  await expect(nameOf('cccc-83IP')).toHaveClass(/id-l2/)
  await expect(nameOf('Sa1b-83IP')).toHaveClass(/id-l2/)
  await expect(nameOf('7bb6-ZZZZ')).not.toHaveClass(/id-l/)
})

test('a suffix containing "/" becomes a badge, is listed, and is sent to NG as it is', async ({
  page,
}) => {
  const addRequests = []
  await gotoSuffixGroup(page)
  await page.route('**/api/ng-wacchoi', async (route) => {
    if (route.request().method() === 'POST') {
      addRequests.push(route.request().postDataJSON())
      route.fulfill({ json: { ok: true } })
    } else {
      route.fulfill({ json: [] })
    }
  })

  const badge = page.locator('.wacchoi-badge[data-wacchoi="86fb-/dMI"]')
  await expect(badge).toHaveText('86fb-/dMI')
  await badge.click()
  await expect(page.locator('.modal')).toContainText('ﾜｯﾁｮｲ末尾:/dMI（2件）')
  await page.getByRole('button', { name: '閉じる' }).click()

  await badge.click({ button: 'right' })
  await page.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }).click()
  await expect.poll(() => addRequests.length).toBe(1)
  expect(addRequests[0]).toEqual({
    suffix: '/dMI',
    board: 'applism',
    week_key: '2024/12/29',
    wacchoi: '86fb-/dMI',
  })
})

test('an NG entry for a "/" suffix hides both prefixes and can be removed', async ({ page }) => {
  const ngWacchoi = [
    {
      suffix: '/dMI',
      board: 'applism',
      week_key: '2024/12/29',
      wacchoi: '86fb-/dMI',
      created_at: 0,
    },
  ]
  const deleteRequests = []
  await gotoSuffixGroup(page, { ngWacchoi })
  await page.route(/\/api\/ng-wacchoi(?:\?.*)?$/, async (route) => {
    if (route.request().method() === 'DELETE') {
      deleteRequests.push(Object.fromEntries(new URL(route.request().url()).searchParams))
      await route.fulfill({ json: { ok: true } })
    } else {
      await route.fulfill({ json: ngWacchoi })
    }
  })

  await expect(page.getByText('スラッシュA')).toHaveCount(0)
  await expect(page.getByText('スラッシュB')).toHaveCount(0)
  const ngHeaders = page.locator('del.ng').filter({ hasText: 'NGワッチョイ' })
  await expect(ngHeaders).toHaveCount(2)

  await ngHeaders.first().click({ button: 'right' })
  await page.getByRole('button', { name: 'NGワッチョイから削除' }).click()
  await expect.poll(() => deleteRequests.length).toBe(1)
  expect(deleteRequests[0]).toEqual({ suffix: '/dMI', board: 'applism', week_key: '2024/12/29' })
})
