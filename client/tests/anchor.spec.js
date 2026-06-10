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

// Res 2 anchors to res 1 (>>1). The body is server-sanitized, so >> is &gt;&gt;.
function datResponse() {
  return {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025 ID:a', body: '最初のレス' },
      { num: 2, name: '名無し', mail: '', date: '2025 ID:b', body: '&gt;&gt;1 これはアンカー' },
    ],
  }
}

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
