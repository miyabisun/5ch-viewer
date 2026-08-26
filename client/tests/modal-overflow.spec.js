import { test, expect } from '@playwright/test'

// Regression: on narrow (phone) viewports, modal content used a viewport-based
// min-width (min(28rem/32rem, 90vw)) that ignored the scrim + modal padding
// (16px x2 each), so content overflowed horizontally. Every modal's content must
// now fit inside the modal's effective width with no horizontal scroll.

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'モーダルオーバーフローテストスレ',
  res_count: 6,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// Long unbreakable tokens (ID / URL) stress word-break; a deep >>N backref chain
// (res2>>1, res3>>2, ...) stresses the anchor-tree indentation.
const LONG_ID = 'verylongidAAAAAAAAAAAAAAAAAAAA'
const LONG_URL = 'https://example.com/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
const WACCHOI_NAME = 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>'

function datResponse() {
  return {
    title: FAV.title,
    res_count: 6,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: WACCHOI_NAME,
        mail: '',
        date: `2025/01/01(水) 00:00:00.00 ID:${LONG_ID}`,
        body: `本文1 ${LONG_URL}`,
        id: LONG_ID,
      },
      {
        num: 2,
        name: WACCHOI_NAME,
        mail: '',
        date: `2025/01/01(水) 00:01:00.00 ID:${LONG_ID}`,
        body: '&gt;&gt;1 本文2',
        id: LONG_ID,
      },
      {
        num: 3,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:02:00.00 ID:c',
        body: '&gt;&gt;2 本文3',
        id: 'c',
      },
      {
        num: 4,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:03:00.00 ID:d',
        body: '&gt;&gt;3 本文4',
        id: 'd',
      },
      {
        num: 5,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:04:00.00 ID:e',
        body: '&gt;&gt;4 本文5',
        id: 'e',
      },
      {
        num: 6,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:05:00.00 ID:f',
        body: `&gt;&gt;5 ${LONG_URL}`,
        id: 'f',
      },
    ],
  }
}

function searchResult() {
  return [
    {
      thread_id: '1771127145',
      title: 'とても長いスレタイトルxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx',
      res: [
        {
          num: 1,
          name: '名無し',
          mail: '',
          date: `2025/01/01 ID:${LONG_ID}`,
          body: `検索ヒット本文 ${LONG_URL}`,
          id: LONG_ID,
        },
      ],
    },
  ]
}

async function setupRoutes(page) {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 6, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-words(\?|$)/, (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route('**/api/ng-wacchoi', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-wacchoi\/.*/, (route) => route.fulfill({ json: { ok: true } }))
  await page.route(/\/api\/boards\/.+\/id-search/, (route) =>
    route.fulfill({ json: searchResult() }),
  )
  await page.route(/\/api\/boards\/.+\/wacchoi-search/, (route) =>
    route.fulfill({ json: searchResult() }),
  )
}

// Assert no horizontal overflow: the modal, its scroll container, and the document
// must not scroll horizontally, and every focusable field's right edge must sit
// inside the viewport.
async function expectNoHorizontalOverflow(page) {
  const modal = page.locator('.modal')
  await expect(modal).toBeVisible()

  const metrics = await page.evaluate(() => {
    const doc = document.documentElement
    const m = document.querySelector('.modal')
    const c = document.querySelector('.modal-content')
    return {
      innerWidth: window.innerWidth,
      docScroll: doc.scrollWidth,
      docClient: doc.clientWidth,
      modalScroll: m.scrollWidth,
      modalClient: m.clientWidth,
      contentScroll: c.scrollWidth,
      contentClient: c.clientWidth,
    }
  })

  // Allow 1px slack for sub-pixel rounding.
  expect(metrics.docScroll).toBeLessThanOrEqual(metrics.docClient + 1)
  expect(metrics.modalScroll).toBeLessThanOrEqual(metrics.modalClient + 1)
  expect(metrics.contentScroll).toBeLessThanOrEqual(metrics.contentClient + 1)

  // Every field/button inside the modal must have its right edge within the viewport.
  const fields = modal.locator('input, textarea, button')
  const count = await fields.count()
  for (let i = 0; i < count; i++) {
    const right = await fields.nth(i).evaluate((el) => el.getBoundingClientRect().right)
    expect(right).toBeLessThanOrEqual(metrics.innerWidth + 1)
  }
}

test.describe('phone viewport (390x700): no modal has horizontal overflow', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 700 })
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1')).toBeVisible()
  })

  test('write (post) modal', async ({ page }) => {
    await page.getByRole('button', { name: '書き込む' }).click()
    await expect(page.locator('.post-form')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('id-list modal (long ID)', async ({ page }) => {
    await page.locator('.resid').first().click()
    await expect(page.locator('[data-testid="id-list"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('id-menu modal', async ({ page }) => {
    await page.locator('.resid').first().click({ button: 'right' })
    await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('id-search-result modal (long URL / long title)', async ({ page }) => {
    await page.locator('.resid').first().click({ button: 'right' })
    await page.getByRole('button', { name: '取得済みスレから検索' }).click()
    await expect(page.locator('[data-testid="id-search-result"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('wacchoi-list modal', async ({ page }) => {
    await page.locator('.wacchoi-badge').first().click()
    await expect(page.locator('[data-testid="wacchoi-list"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('wacchoi-menu modal', async ({ page }) => {
    await page.locator('.wacchoi-badge').first().click({ button: 'right' })
    await expect(page.locator('[data-testid="wacchoi-menu"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('wacchoi-search-result modal', async ({ page }) => {
    await page.locator('.wacchoi-badge').first().click({ button: 'right' })
    await page.getByRole('button', { name: '取得済みスレから検索' }).click()
    await expect(page.locator('[data-testid="wacchoi-search-result"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('ng-word modal (segmented control + textarea + two actions)', async ({ page }) => {
    await page.getByText('本文1').click({ button: 'right' })
    await page.getByRole('button', { name: 'NG Word に追加' }).click()
    await expect(page.locator('[data-testid="ng-word-form"]')).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('anchor-tree modal (deep >>N chain + long URL)', async ({ page }) => {
    // Click >>1 to build the deepest chain (res2>>1, res3>>2, ... res6>>5).
    await page.locator('.anchor[data-anchor="1"]').first().click()
    await expect(page.locator('.modal')).toBeVisible()
    await expect(page.locator('.anchor-node').first()).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })
})

test('desktop viewport (1024px): post-form keeps a comfortable width', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 800 })
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  await page.getByRole('button', { name: '書き込む' }).click()
  const form = page.locator('.post-form')
  await expect(form).toBeVisible()

  // On desktop the form must not collapse to the mobile width: it should be
  // close to its 28rem (~448px) design width.
  const width = await form.evaluate((el) => el.getBoundingClientRect().width)
  expect(width).toBeGreaterThan(400)
})
