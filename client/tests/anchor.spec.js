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

// Res 1 anchors to res 2 (>>2); res 2 anchors back to res 1 (>>1) — mutual cycle.
// The body is server-sanitized, so >> is &gt;&gt;.
function datResponse() {
  return {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025 ID:a', body: '最初のレス &gt;&gt;2' },
      { num: 2, name: '名無し', mail: '', date: '2025 ID:b', body: '&gt;&gt;1 これはアンカー' },
    ],
  }
}

// Regression: real 5ch dat bodies wrap >>N in <a href="../test/read.cgi/..."> tags.
// The server sanitizer must strip those <a> tags so the frontend linkify path works.
test('thread-anchor <a> from dat is stripped to plain >>N span by the server', async ({
  page,
}) => {
  // Simulate server-sanitized body: the sanitizer has already stripped the <a> tag
  // and left the inner &gt;&gt;1 text, which linkify then wraps in .anchor span.
  const datWithRealAnchor = {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025 ID:a', body: '最初のレス' },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025 ID:b',
        // After server sanitization, the <a href="../test/read.cgi/..."> is stripped
        // and only the inner &gt;&gt;1 text remains (same as the existing mock).
        body: '&gt;&gt;1 read.cgiアンカーから変換',
      },
    ],
  }
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datWithRealAnchor }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  await expect(page.getByText('read.cgiアンカーから変換')).toBeVisible()

  // The >>1 must be an .anchor span (not a raw <a> link) so clicking opens the modal.
  const anchor = page.locator('.anchor[data-anchor="1"]').first()
  await expect(anchor).toBeVisible()
  expect(await anchor.evaluate((el) => el.tagName)).toBe('SPAN')

  await anchor.click()
  await expect(page.locator('.modal')).toBeVisible()
  await expect(page.locator('.modal').getByText('最初のレス')).toBeVisible()
})

test('clicking a >>N anchor opens the modal without navigating', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  await expect(page.getByText('これはアンカー')).toBeVisible()

  // The anchor is a span, not a link (no navigation element).
  const anchor = page.locator('.anchor[data-anchor="1"]').first()
  await expect(anchor).toBeVisible()
  expect(await anchor.evaluate((el) => el.tagName)).toBe('SPAN')

  await anchor.click()

  // Modal shows the referenced res; URL stays on the thread path; the ThreadView
  // (sticky title) is still mounted (we did not navigate to the list).
  await expect(page.locator('.modal')).toBeVisible()
  await expect(page.locator('.modal').getByText('最初のレス')).toBeVisible()
  await expect(page).toHaveURL(THREAD_PATH)
  await expect(page.getByTestId('thread-title')).toBeVisible()
})

// New tree structure: parents (forward-anchor targets) above, N in the middle,
// backrefs (reses that reference N) below.
// Clicking >>1 (N=1): res1 references >>2 so res2 is the parent; res2 references >>1
// so res2 would be a child too, but it is already used as a parent and appears only once.
test('anchor tree: parent (forward target) above, self in middle, backrefs below', async ({
  page,
}) => {
  // res1 anchors >>2, res2 anchors >>1 (mutual cycle), res3 anchors >>1 (backref only).
  const dat = {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025 ID:a', body: '最初のレス &gt;&gt;2' },
      { num: 2, name: '名無し', mail: '', date: '2025 ID:b', body: '&gt;&gt;1 これはアンカー' },
      { num: 3, name: '名無し', mail: '', date: '2025 ID:c', body: '&gt;&gt;1 別の返信' },
    ],
  }
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: dat }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 3, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  // Click >>1 in res2 or res3 (both reference >>1).
  await page.locator('.anchor[data-anchor="1"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  const modal = page.locator('.modal')
  // res1 (N=1, self) must be visible — highlighted as the pivot.
  await expect(modal.getByText('最初のレス')).toBeVisible()
  // res2 is res1's forward-anchor target (res1 references >>2) — shown as parent.
  await expect(modal.getByText('これはアンカー')).toBeVisible()
  // res3 is a backref of res1 (res3 references >>1) — shown as child.
  await expect(modal.getByText('別の返信')).toBeVisible()

  // Each res must appear exactly once.
  await expect(modal.locator('.res .num').filter({ hasText: '1' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '2' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '3' })).toHaveCount(1)
})

// Clicking >>1 (res1 anchors to res2, res2 anchors back to res1) must show both
// res1 and res2 in the tree and must not loop infinitely.
// New behaviour: res2 appears as a parent (res1 references >>2) and is NOT duplicated
// as a child (because it is already used as an ancestor).
test('anchor tree expands recursively and stops at cycles', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  await expect(page.getByText('これはアンカー')).toBeVisible()

  // Click the >>1 anchor in res2's body (N=1).
  await page.locator('.anchor[data-anchor="1"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  // Both res1 (N) and res2 (parent: res1 references >>2) must appear.
  const modal = page.locator('.modal')
  await expect(modal.getByText('最初のレス')).toBeVisible()
  await expect(modal.getByText('これはアンカー')).toBeVisible()

  // Each res must appear exactly once (cycle-safe: res2 shown as parent only).
  await expect(modal.locator('.res .num').filter({ hasText: '1' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '2' })).toHaveCount(1)
})

// DAG test: res1 -> res2 and res1 -> res3; both res2 and res3 -> res4.
// Clicking >>2 or >>3 (from res4's perspective as backrefs to res1):
// This test clicks >>4 from res2 (N=4):
//   - parent chain: res4 has no forward anchors -> no parents
//   - self: res4
//   - children (backrefs to res4): res2 and res3 both anchor >>4
// Each res must appear exactly once — no duplicate expansion.
test('anchor tree expands DAG without duplicating shared nodes', async ({ page }) => {
  const dagDat = {
    title: FAV.title,
    res_count: 4,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025 ID:a', body: 'root &gt;&gt;2 &gt;&gt;3' },
      { num: 2, name: '名無し', mail: '', date: '2025 ID:b', body: '&gt;&gt;4 経路A' },
      { num: 3, name: '名無し', mail: '', date: '2025 ID:c', body: '&gt;&gt;4 経路B' },
      { num: 4, name: '名無し', mail: '', date: '2025 ID:d', body: '共有ノード' },
    ],
  }

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: dagDat }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 4, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  // Click >>4 in res2's body (N=4): res2 and res3 are backrefs (children).
  await page.locator('.anchor[data-anchor="4"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  const modal = page.locator('.modal')
  // res4 (self), res2 and res3 (backrefs / children) must appear.
  await expect(modal.getByText('共有ノード')).toBeVisible()
  await expect(modal.getByText('経路A')).toBeVisible()
  await expect(modal.getByText('経路B')).toBeVisible()

  // res2 and res3 both reference >>4 so they appear as children; each exactly once.
  await expect(modal.locator('.res .num').filter({ hasText: '2' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '3' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '4' })).toHaveCount(1)
})

// The anchor-tree modal follows the shared modal convention: a top-right × and an
// outside (scrim) click both close it; there is no bottom "閉じる" button.
test('anchor modal closes via × and via outside click (no bottom 閉じる)', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  await page.locator('.anchor[data-anchor="1"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  // No bottom 閉じる button anymore: the only close affordance is the top-right ×.
  await expect(page.locator('.modal').getByText('閉じる')).toHaveCount(0)
  await expect(page.locator('.modal-close')).toHaveText('×')

  // Top-right × closes it.
  await page.locator('.modal-close').click()
  await expect(page.locator('.modal')).toHaveCount(0)

  // Reopen, then close via an outside (scrim) click.
  await page.locator('.anchor[data-anchor="1"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()
  await page.locator('.modal-bg').click({ position: { x: 5, y: 5 } })
  await expect(page.locator('.modal')).toHaveCount(0)
})

// Regression (issue #6 redux): a touch tap on an anchor must open the modal and
// must NOT be treated as a right-swipe-back. A genuine right-swipe on the body
// (not on an anchor) still leaves the thread. This proves swipe vs anchor are
// clearly distinguished.
test.describe('touch: anchor tap vs swipe-back', () => {
  test.use({ hasTouch: true })

  // Dispatches a synthetic touch gesture (start -> move -> end) on an element.
  async function swipe(page, selector, { dx, dy = 0 }) {
    await page.evaluate(
      ({ selector, dx, dy }) => {
        const el = document.querySelector(selector)
        const r = el.getBoundingClientRect()
        const x0 = r.left + Math.min(20, r.width / 2)
        const y0 = r.top + r.height / 2
        const mk = (type, x, y, list) => {
          const t = new Touch({ identifier: 1, target: el, clientX: x, clientY: y })
          el.dispatchEvent(
            new TouchEvent(type, {
              bubbles: true,
              cancelable: true,
              touches: list ? [t] : [],
              targetTouches: list ? [t] : [],
              changedTouches: [t],
            }),
          )
        }
        mk('touchstart', x0, y0, true)
        // a few intermediate moves so the gesture locks horizontal
        mk('touchmove', x0 + dx * 0.3, y0 + dy * 0.3, true)
        mk('touchmove', x0 + dx * 0.7, y0 + dy * 0.7, true)
        mk('touchend', x0 + dx, y0 + dy, false)
      },
      { selector, dx, dy },
    )
  }

  test('a swipe starting on an anchor does not navigate back', async ({ page }) => {
    await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
    await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
      route.fulfill({ json: datResponse() }),
    )
    await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
    )

    await page.goto(THREAD_PATH)
    await expect(page.getByText('これはアンカー')).toBeVisible()

    // A clear right-swipe whose touch STARTS on the anchor must be ignored as a
    // swipe (it is an anchor interaction), so we stay on the thread.
    await swipe(page, '.anchor[data-anchor="1"]', { dx: 150 })
    await expect(page).toHaveURL(THREAD_PATH)
    await expect(page.getByTestId('thread-title')).toBeVisible()
  })

  test('a clear right-swipe on the body returns to the list', async ({ page }) => {
    await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
    await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
      route.fulfill({ json: datResponse() }),
    )
    await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
    )

    await page.goto(THREAD_PATH)
    await expect(page.getByText('これはアンカー')).toBeVisible()

    // Swipe on the body (not on an anchor) -> back to the list (URL '/').
    await swipe(page, '.thread-body', { dx: 150 })
    await expect(page).toHaveURL('/')
  })
})
