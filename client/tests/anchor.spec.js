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
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025 ID:a',
        body: '最初のレス &gt;&gt;2',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025 ID:b',
        body: '&gt;&gt;1 これはアンカー',
      },
    ],
  }
}

// Real 5ch dat bodies wrap >>N in <a href="../test/read.cgi/..."> tags; the server
// sanitizer (src/sanitize.rs, covered by strips_thread_anchor_relative_path /
// _absolute_path / strips_multiple_thread_anchors) strips those <a> tags and leaves
// the plain &gt;&gt;N text. This test verifies the frontend side of that pipeline:
// given an already-sanitized body, the >>N text is linkified into a clickable
// .anchor span (not left as plain text or a raw link), and clicking it opens the modal.
test('a sanitized >>N in a dat body renders as a clickable .anchor span (not a raw link)', async ({
  page,
}) => {
  // Body as it looks after server-side sanitization: the <a href="../test/read.cgi/...">
  // has already been stripped, leaving only the inner &gt;&gt;1 text for the frontend
  // to linkify into an .anchor span.
  const datWithRealAnchor = {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025 ID:a',
        body: '最初のレス',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025 ID:b',
        // Already-sanitized body: only the inner &gt;&gt;1 text remains (same shape
        // the server would produce after stripping the <a href="../test/read.cgi/...">).
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
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
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

// Single unified tree: ancestors (shallowest first) -> self -> descendants.
// Clicking >>1 (N=1): res1 references >>2 so res2 is an ancestor; res2 references >>1
// (cycle) so res2 is NOT duplicated as a child — it appears only once as an ancestor.
// res3 references >>1 (backref) so res3 is a descendant (child of self).
//
// Expected single-tree order: res2 (depth 0, ancestor) -> res1 (depth 1, self) -> res3 (depth 2, child)
test('anchor tree: unified single tree with ancestors above self and children below', async ({
  page,
}) => {
  // res1 anchors >>2, res2 anchors >>1 (mutual cycle), res3 anchors >>1 (backref only).
  const dat = {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025 ID:a',
        body: '最初のレス &gt;&gt;2',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025 ID:b',
        body: '&gt;&gt;1 これはアンカー',
      },
      {
        num: 3,
        name: '名無し',
        mail: '',
        date: '2025 ID:c',
        body: '&gt;&gt;1 別の返信',
      },
    ],
  }
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: dat }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 3, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  // Click >>1 in res2 or res3 (both reference >>1).
  await page.locator('.anchor[data-anchor="1"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  const modal = page.locator('.modal')
  // All three reses must be visible.
  await expect(modal.getByText('最初のレス')).toBeVisible()
  await expect(modal.getByText('これはアンカー')).toBeVisible()
  await expect(modal.getByText('別の返信')).toBeVisible()

  // Each res must appear exactly once (res2 cycle-deduplicated as ancestor only).
  await expect(modal.locator('.res .num').filter({ hasText: '1' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '2' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '3' })).toHaveCount(1)

  // self (res1) must be highlighted; ancestor (res2) and child (res3) must not.
  await expect(modal.locator('.res.anchor-self .num').filter({ hasText: '1' })).toHaveCount(1)
  await expect(modal.locator('.res.anchor-self .num').filter({ hasText: '2' })).toHaveCount(0)
  await expect(modal.locator('.res.anchor-self .num').filter({ hasText: '3' })).toHaveCount(0)
})

// Clicking >>1 (res1 anchors to res2, res2 anchors back to res1) must show both
// res1 and res2 in the tree and must not loop infinitely.
// New behaviour: res2 appears as a parent (res1 references >>2) and is NOT duplicated
// as a child (because it is already used as an ancestor).
test('anchor tree expands recursively and stops at cycles', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
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
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025 ID:a',
        body: 'root &gt;&gt;2 &gt;&gt;3',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025 ID:b',
        body: '&gt;&gt;4 経路A',
      },
      {
        num: 3,
        name: '名無し',
        mail: '',
        date: '2025 ID:c',
        body: '&gt;&gt;4 経路B',
      },
      {
        num: 4,
        name: '名無し',
        mail: '',
        date: '2025 ID:d',
        body: '共有ノード',
      },
    ],
  }

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: dagDat }))
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
test('anchor modal closes via × and via outside click (no bottom 閉じる)', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  await page.locator('.anchor[data-anchor="1"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  // No bottom 閉じる button anymore: the only close affordance is the top-right ×.
  await expect(page.locator('.modal').getByText('閉じる')).toHaveCount(0)
  // The close affordance is a quiet icon button with an SVG x (DESIGN.md), not a text glyph.
  await expect(page.locator('.modal-close svg')).toBeVisible()
  await expect(page.locator('.modal-close')).toHaveText('')

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
          const t = new Touch({
            identifier: 1,
            target: el,
            clientX: x,
            clientY: y,
          })
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
    await page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
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
    await page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
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

// Regression: 3-res linear chain (859 -> 863 -> 886 equivalent).
// Clicking the middle res (863) must produce a single tree where:
//   ancestor (859) is shallowest (leftmost), self (863) is middle, child (886) is deepest.
// The left positions must be strictly monotonically increasing: left(859) < left(863) < left(886).
test('anchor tree depth indentation: ancestor shallower than self, self shallower than child', async ({
  page,
}) => {
  // Three-res linear chain: res859 <- res863(self) <- res886
  const dat = {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 859,
        name: '名無し',
        mail: '',
        date: '2025 ID:a',
        body: '859のレス',
      },
      {
        num: 863,
        name: '名無し',
        mail: '',
        date: '2025 ID:b',
        body: '&gt;&gt;859 863のレス',
      },
      {
        num: 886,
        name: '名無し',
        mail: '',
        date: '2025 ID:c',
        body: '&gt;&gt;863 886のレス',
      },
    ],
  }
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: dat }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 3, read_res: 0, status: 'active' } }),
  )

  await page.goto(THREAD_PATH)
  // Click >>863 anchor in res886's body (opens tree with 863 as self).
  await page.locator('.anchor[data-anchor="863"]').first().click()
  await expect(page.locator('.modal')).toBeVisible()

  const modal = page.locator('.modal')
  await expect(modal.getByText('859のレス')).toBeVisible()
  await expect(modal.getByText('863のレス')).toBeVisible()
  await expect(modal.getByText('886のレス')).toBeVisible()

  // Each res appears exactly once.
  await expect(modal.locator('.res .num').filter({ hasText: '859' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '863' })).toHaveCount(1)
  await expect(modal.locator('.res .num').filter({ hasText: '886' })).toHaveCount(1)

  // self (863) must be highlighted; ancestor and child must not.
  await expect(modal.locator('.res.anchor-self .num').filter({ hasText: '863' })).toHaveCount(1)
  await expect(modal.locator('.res.anchor-self .num').filter({ hasText: '859' })).toHaveCount(0)
  await expect(modal.locator('.res.anchor-self .num').filter({ hasText: '886' })).toHaveCount(0)

  // Key correctness check: left positions must be strictly monotonically increasing.
  // ancestor(859).left < self(863).left < child(886).left
  const [left859, left863, left886] = await modal.evaluate(() => {
    function resLeft(numText) {
      for (const el of document.querySelectorAll('.modal .res')) {
        const numEl = el.querySelector('.num')
        if (numEl && numEl.textContent.trim() === numText) {
          return el.getBoundingClientRect().left
        }
      }
      return null
    }
    return [resLeft('859'), resLeft('863'), resLeft('886')]
  })

  expect(left859).not.toBeNull()
  expect(left863).not.toBeNull()
  expect(left886).not.toBeNull()
  // Ancestor must be strictly to the left of self.
  expect(left859).toBeLessThan(left863)
  // Self must be strictly to the left of child.
  expect(left863).toBeLessThan(left886)
})
