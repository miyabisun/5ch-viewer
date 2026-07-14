import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'NGテストスレ',
  res_count: 3,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// Two posts share ID:target (appears twice -> badge shown), one post has a different ID.
function datResponse(ngIds = []) {
  return {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025/01/01(水) 00:00:00.00 ID:target', body: '本文1', id: 'target' },
      { num: 2, name: '名無し', mail: '', date: '2025/01/01(水) 00:01:00.00 ID:other', body: '本文2', id: 'other' },
      { num: 3, name: '名無し', mail: '', date: '2025/01/01(水) 00:02:00.00 ID:target', body: '本文3', id: 'target' },
    ],
  }
}

// Standard route setup shared across tests.
async function setupRoutes(page, { ngIds = [], searchResult = [] } = {}) {
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
      route.fulfill({ json: ngIds.map((ng_id) => ({ ng_id, created_at: 0 })) })
    } else {
      route.fulfill({ json: { ok: true } })
    }
  })
  await page.route(/\/api\/ng-ids\/.*/, (route) =>
    route.fulfill({ json: { ok: true } }),
  )
  await page.route(/\/api\/boards\/.+\/id-search/, (route) =>
    route.fulfill({ json: searchResult }),
  )
}

test('left-click on ID badge opens id-list modal with all reses of that ID', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  // Wait for thread to render.
  await expect(page.getByText('本文1')).toBeVisible()

  // Left-click the first .resid badge (ID:target, which has 2 posts).
  await page.locator('.resid').first().click()

  // The id-list modal must appear.
  const modal = page.locator('[data-testid="id-list"]')
  await expect(modal).toBeVisible()

  // Both reses posted by ID:target must appear in the modal.
  await expect(modal.getByText('本文1')).toBeVisible()
  await expect(modal.getByText('本文3')).toBeVisible()
  // The other ID's post must not appear.
  await expect(modal.getByText('本文2')).toHaveCount(0)
})

test('id-list modal header shows ID and count', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  await page.locator('.resid').first().click()

  // Header should read "ID:target（2件）".
  await expect(page.locator('.modal')).toContainText('ID:target（2件）')
})

test('left-click opens id-list; right-click opens id-menu (both routes distinct)', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()

  // Left-click -> id-list modal.
  await page.locator('.resid').first().click()
  await expect(page.locator('[data-testid="id-list"]')).toBeVisible()
  await expect(page.locator('[data-testid="id-menu"]')).toHaveCount(0)

  // Close the modal.
  await page.getByRole('button', { name: '閉じる' }).click()
  await expect(page.locator('[data-testid="id-list"]')).toHaveCount(0)

  // Right-click -> id-menu modal.
  await page.locator('.resid').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()
  await expect(page.locator('[data-testid="id-list"]')).toHaveCount(0)
})

test('right-click on ID badge opens the ID menu', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  // Wait for the thread body to render.
  await expect(page.getByText('本文1')).toBeVisible()

  // The ID badge shows for "target" (appears twice -> stats.total >= 2).
  const badge = page.locator('.resid').first()
  await expect(badge).toBeVisible()

  // Right-click opens the ID menu.
  await badge.click({ button: 'right' })
  await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()

  // Menu contains the three expected actions.
  await expect(page.getByRole('button', { name: 'NGIDに追加' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'コピー' })).toBeVisible()
  await expect(page.getByRole('button', { name: '取得済みスレから検索' })).toBeVisible()
})

test('NG post: reason header is struck-through and click toggles the body', async ({ page }) => {
  // Simulate "target" already in the NG list.
  await setupRoutes(page, { ngIds: ['target'] })
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toHaveCount(0)
  await expect(page.getByText('本文3')).toHaveCount(0)
  // Non-NG post body is still shown.
  await expect(page.getByText('本文2')).toBeVisible()

  // The NG res has the concise reason header in the del.ng wrapper.
  const ngDel = page.locator('del.ng')
  await expect(ngDel).toHaveCount(2) // two posts with ID:target
  const res1Ng = ngDel.filter({ hasText: '[レス番号: 1]' })
  await expect(res1Ng).toHaveText('[レス番号: 1] [理由: NG ID]')

  // Clicking the replacement header reveals the original header and body.
  await res1Ng.click()
  await expect(page.getByText('本文1')).toBeVisible()
  await expect(page.getByText('本文3')).toHaveCount(0)
  // The privacy-preserving reason header remains in place; only the body toggles.
  await expect(page.locator('.res[data-res="1"]').getByText('ID:target')).toHaveCount(0)

  // Clicking it again restores the initial hidden state.
  await res1Ng.click()
  await expect(page.getByText('本文1')).toHaveCount(0)
})

test('NGID追加 adds ID to NG and the ID menu does not expose removal', async ({ page }) => {
  const addRequests = []
  await setupRoutes(page)
  await page.route('**/api/ng-ids', async (route) => {
    if (route.request().method() === 'POST') {
      addRequests.push(route.request().postDataJSON())
      route.fulfill({ json: { ok: true } })
    } else {
      // First GET returns empty; second GET (after add) returns the new ID.
      const call = addRequests.length
      route.fulfill({
        json: call > 0 ? [{ ng_id: 'target', created_at: 0 }] : [],
      })
    }
  })

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  const badge = page.locator('.resid').first()
  await badge.click({ button: 'right' })
  await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()

  await page.getByRole('button', { name: 'NGIDに追加' }).click()

  // The API was called.
  await expect.poll(() => addRequests.length).toBe(1)
  expect(addRequests[0]).toEqual({ ng_id: 'target' })

  // Removal is available from the NG post menu instead of the now-hidden ID badge.
  await expect(page.locator('del.ng').first()).toBeVisible()
  await expect(page.locator('.resid[data-id="target"]')).toHaveCount(0)
  await expect(page.getByRole('button', { name: 'NGIDから削除' })).toHaveCount(0)
})

test('NG post right-click opens its reason menu and removes the NG ID', async ({ page }) => {
  const removeRequests = []
  await setupRoutes(page, { ngIds: ['target'] })
  await page.route(/\/api\/ng-ids\/.*/, async (route) => {
    if (route.request().method() === 'DELETE') {
      removeRequests.push(route.request().url())
    }
    await route.fulfill({ json: { ok: true } })
  })

  await page.goto(THREAD_PATH)
  const ngHeader = page.locator('del.ng').filter({ hasText: '[レス番号: 1]' })
  await expect(ngHeader).toHaveText('[レス番号: 1] [理由: NG ID]')

  await ngHeader.click({ button: 'right' })
  await expect(page.locator('[data-testid="ng-menu"]')).toBeVisible()
  await expect(page.getByRole('button', { name: 'NG IDから削除' })).toBeVisible()
  await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)

  await page.getByRole('button', { name: 'NG IDから削除' }).click()
  await expect.poll(() => removeRequests.length).toBe(1)
  expect(decodeURIComponent(new URL(removeRequests[0]).pathname)).toBe('/api/ng-ids/target')
})

test('NG post long-press opens only its reason menu without toggling the body', async ({ page }) => {
  await setupRoutes(page, { ngIds: ['target'] })
  await page.goto(THREAD_PATH)

  const ngHeader = page.locator('del.ng').first()
  await ngHeader.dispatchEvent('pointerdown', { pointerType: 'touch', pointerId: 1 })
  await page.waitForTimeout(550)
  await ngHeader.dispatchEvent('pointerup', { pointerType: 'touch', pointerId: 1 })
  await ngHeader.dispatchEvent('click')

  await expect(page.locator('[data-testid="ng-menu"]')).toBeVisible()
  await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)
  await expect(page.getByText('本文1')).toHaveCount(0)
})

test('copy writes "ID:xxx" to clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  const badge = page.locator('.resid').first()
  await badge.click({ button: 'right' })
  await page.getByRole('button', { name: 'コピー' }).click()

  const text = await page.evaluate(() => navigator.clipboard.readText())
  expect(text).toBe('ID:target')
})

test('id-search shows results modal', async ({ page }) => {
  const searchResult = [
    {
      thread_id: '1771127145',
      title: 'NGテストスレ',
      res: [
        { num: 1, name: '名無し', mail: '', date: '2025/01/01 ID:target', body: '検索ヒット本文', id: 'target' },
      ],
    },
  ]
  await setupRoutes(page, { searchResult })
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  const badge = page.locator('.resid').first()
  await badge.click({ button: 'right' })
  await page.getByRole('button', { name: '取得済みスレから検索' }).click()

  // Search result modal appears.
  const resultModal = page.locator('[data-testid="id-search-result"]')
  await expect(resultModal).toBeVisible()
  await expect(page.getByText('検索ヒット本文')).toBeVisible()
})

test('id-search shows 該当なし when no results', async ({ page }) => {
  await setupRoutes(page, { searchResult: [] })
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  const badge = page.locator('.resid').first()
  await badge.click({ button: 'right' })
  await page.getByRole('button', { name: '取得済みスレから検索' }).click()

  await expect(page.locator('[data-testid="id-search-result"]')).toBeVisible()
  await expect(page.getByText('該当なし')).toBeVisible()
})

test('ID menu closes via × button', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)

  await expect(page.getByText('本文1')).toBeVisible()
  const badge = page.locator('.resid').first()
  await badge.click({ button: 'right' })
  await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()

  await page.getByRole('button', { name: '閉じる' }).click()
  await expect(page.locator('[data-testid="id-menu"]')).toHaveCount(0)
})

// Regression (review warning): NG body must be hidden even when the res is
// opened via an anchor tree (>>N click). Previously anchorNode called
// resHead+body directly and bypassed NG filtering, allowing the body to show.
test('NG body is hidden inside the anchor tree (anchorNode uses resHeadAndBody)', async ({
  page,
}) => {
  // res1 anchors to res2 (NG); res2 body must not appear in the anchor tree.
  const datWithAnchor = {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00 ID:normal',
        body: 'アンカー &gt;&gt;2',
        id: 'normal',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:01:00.00 ID:target',
        body: 'NG本文がここに表示されてはいけない',
        id: 'target',
      },
    ],
  }

  // Register "target" as NG so res2 body should be hidden everywhere.
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datWithAnchor }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ json: [{ ng_id: 'target', created_at: 0 }] })
    } else {
      route.fulfill({ json: { ok: true } })
    }
  })
  await page.route(/\/api\/ng-ids\/.*/, (route) =>
    route.fulfill({ json: { ok: true } }),
  )
  await page.route(/\/api\/boards\/.+\/id-search/, (route) =>
    route.fulfill({ json: [] }),
  )

  await page.goto(THREAD_PATH)

  // The anchor >>2 is rendered in res1's body (server sends &gt;&gt;2).
  await expect(page.locator('.anchor[data-anchor="2"]')).toBeVisible()

  // Click the >>2 anchor to open the anchor-tree modal.
  await page.locator('.anchor[data-anchor="2"]').click()
  await expect(page.locator('.modal')).toBeVisible()

  // The NG body text must NOT appear anywhere in the modal.
  await expect(
    page.locator('.modal').getByText('NG本文がここに表示されてはいけない'),
  ).toHaveCount(0)

  // The NG res header (del.ng) must appear inside the modal (struck-through header is shown).
  await expect(page.locator('.modal del.ng')).toHaveCount(1)
})

// Regression (review warning): single-occurrence ID (total=1) must also show a
// clickable ID element so Copy / Search / NGID-add are accessible for any post.
test('single-occurrence ID shows a clickable resid span', async ({ page }) => {
  // Use a dat where each post has a unique ID (all total=1).
  const datUniqueIds = {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025/01/01 00:00:00 ID:uniq1', body: '本文A', id: 'uniq1' },
      { num: 2, name: '名無し', mail: '', date: '2025/01/01 00:01:00 ID:uniq2', body: '本文B', id: 'uniq2' },
    ],
  }

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datUniqueIds }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
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
  await page.route(/\/api\/boards\/.+\/id-search/, (route) =>
    route.fulfill({ json: [] }),
  )

  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文A')).toBeVisible()

  // Both single-occurrence IDs must have a .resid span (not hidden).
  const badges = page.locator('.resid')
  await expect(badges).toHaveCount(2)

  // Right-clicking the first badge opens the ID menu.
  await badges.first().click({ button: 'right' })
  await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()
  await expect(page.getByRole('button', { name: 'NGIDに追加' })).toBeVisible()
})
