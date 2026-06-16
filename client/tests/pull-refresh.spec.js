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
  page.route('**/api/favorites', (route) =>
    route.fulfill({ json: [FAV] }),
  )
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
      route.fulfill({ json: { res_count: datCount, read_res: 0, status: 'active' } }),
    )
  }
}

// Dispatches a synthetic touch swipe on the element at `selector`.
// dx/dy are the total displacement from start to end.
// Intermediate moves are injected so direction-lock code can compute |dx| vs |dy|.
// The start point is clamped to the visible viewport so events are always dispatched
// on a rendered pixel (getBoundingClientRect can return negative top when scrolled).
async function swipe(page, selector, { dx = 0, dy = 0 }) {
  await page.evaluate(
    ({ selector, dx, dy }) => {
      const el = document.querySelector(selector)
      // Start in the centre of the viewport (always visible) so that touch events
      // land on a rendered pixel even when the element itself is partly off-screen.
      const x0 = window.innerWidth / 2
      const y0 = window.innerHeight / 2
      const mk = (type, x, y, active) => {
        const t = new Touch({ identifier: 1, target: el, clientX: x, clientY: y })
        el.dispatchEvent(
          new TouchEvent(type, {
            bubbles: true,
            cancelable: true,
            touches: active ? [t] : [],
            targetTouches: active ? [t] : [],
            changedTouches: [t],
          }),
        )
      }
      mk('touchstart', x0, y0, true)
      mk('touchmove', x0 + dx * 0.3, y0 + dy * 0.3, true)
      mk('touchmove', x0 + dx * 0.7, y0 + dy * 0.7, true)
      mk('touchend', x0 + dx, y0 + dy, false)
    },
    { selector, dx, dy },
  )
}

// Programmatically scroll the appropriate scroll container to its bottom.
async function scrollToBottom(page) {
  // Check whether we are on PC (>=768px) or phone layout.
  const isPC = await page.evaluate(() => window.matchMedia('(min-width: 768px)').matches)
  if (isPC) {
    await page.locator('.detail-pane').evaluate((el) => el.scrollTo(0, el.scrollHeight))
  } else {
    await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight))
  }
}

// ─── overscroll-behavior tests ──────────────────────────────────────────────

test.describe('overscroll-behavior: wall', () => {
  test.use({ viewport: { width: 1024, height: 800 } })

  test('PC detail-pane has overscroll-behavior:contain', async ({ page }) => {
    await mockRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    const value = await page.locator('.detail-pane').evaluate((el) =>
      getComputedStyle(el).overscrollBehavior,
    )
    // contain or contain contain (shorthand)
    expect(value).toMatch(/contain/)
  })

  test('html/body has overscroll-behavior-y:contain (mobile wall)', async ({ page }) => {
    await mockRoutes(page)
    await page.goto(THREAD_PATH)

    const htmlValue = await page.evaluate(() =>
      getComputedStyle(document.documentElement).overscrollBehaviorY,
    )
    const bodyValue = await page.evaluate(() =>
      getComputedStyle(document.body).overscrollBehaviorY,
    )
    expect(htmlValue).toBe('contain')
    expect(bodyValue).toBe('contain')
  })
})

// ─── Pull-to-refresh gesture tests (touch-only) ─────────────────────────────

test.describe('pull-to-refresh (touch)', () => {
  test.use({ hasTouch: true, viewport: { width: 390, height: 700 } })

  test('pull-refresh panel is present in DOM when data is loaded', async ({ page }) => {
    await mockRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()
    // Panel exists (hidden below viewport via transform, but in DOM).
    await expect(page.getByTestId('pull-refresh')).toBeAttached()
  })

  test('upward swipe at bottom immediately after arriving does NOT trigger refresh (0.5 s lock)', async ({
    page,
  }) => {
    let reloadCount = 0
    await mockRoutes(page, {
      reloadHandler: (route) => {
        reloadCount++
        route.fulfill({ json: { res_count: 40, read_res: 0, status: 'active' } })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    // Reset count after initial auto-load.
    const countBefore = reloadCount

    // Scroll to the very bottom so isAtBottom() returns true.
    await scrollToBottom(page)

    // Immediately swipe up (dy < 0) WITHOUT waiting 500 ms — should be blocked.
    await swipe(page, '.thread-body', { dy: -120 })

    // Small wait to ensure any async reload would have fired.
    await page.waitForTimeout(100)
    expect(reloadCount).toBe(countBefore)
  })

  test('upward swipe at bottom after 0.5 s unlock triggers reload', async ({ page }) => {
    let reloadCount = 0
    await mockRoutes(page, {
      reloadHandler: (route) => {
        reloadCount++
        route.fulfill({ json: { res_count: 40, read_res: 0, status: 'active' } })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    // Scroll to the bottom to arm the unlock timer.
    await scrollToBottom(page)

    // Wait for the 0.5 s lock to expire.
    await page.waitForTimeout(600)

    const countBefore = reloadCount

    // Swipe up past the PULL_THRESHOLD_PX (80 px).
    await swipe(page, '.thread-body', { dy: -120 })

    // Reload should have fired.
    await page.waitForTimeout(200)
    expect(reloadCount).toBeGreaterThan(countBefore)
  })

  test('short threads (fits in one screen) never arm the gesture', async ({ page }) => {
    let reloadCount = 0
    // Only 1 res → thread fits in one viewport height, never scrollable.
    await mockRoutes(page, {
      datCount: 1,
      reloadHandler: (route) => {
        reloadCount++
        route.fulfill({ json: { res_count: 1, read_res: 0, status: 'active' } })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    const countBefore = reloadCount

    // Wait well past unlock time.
    await page.waitForTimeout(700)

    // Swipe up — should not trigger because scrollHeight <= clientHeight.
    await swipe(page, '.thread-body', { dy: -120 })
    await page.waitForTimeout(200)
    expect(reloadCount).toBe(countBefore)
  })

  test('horizontal swipe at bottom does not trigger pull-to-refresh', async ({ page }) => {
    let reloadCount = 0
    await mockRoutes(page, {
      reloadHandler: (route) => {
        reloadCount++
        route.fulfill({ json: { res_count: 40, read_res: 0, status: 'active' } })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    await scrollToBottom(page)
    await page.waitForTimeout(600)

    const countBefore = reloadCount

    // Clear horizontal swipe (dx >> dy).
    await swipe(page, '.thread-body', { dx: 150, dy: -5 })
    await page.waitForTimeout(200)
    expect(reloadCount).toBe(countBefore)
  })

  test('no double-fire: second gesture while refreshing is ignored', async ({ page }) => {
    let reloadCount = 0
    // Make reload take 800 ms so we can fire a second gesture while it is still in flight.
    await mockRoutes(page, {
      reloadHandler: async (route) => {
        reloadCount++
        await new Promise((r) => setTimeout(r, 800))
        route.fulfill({ json: { res_count: 40, read_res: 0, status: 'active' } })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    await scrollToBottom(page)
    await page.waitForTimeout(600)

    const countBefore = reloadCount

    // Fire first gesture — starts refresh (800 ms in flight).
    await swipe(page, '.thread-body', { dy: -120 })
    // Wait just a moment (much less than 800 ms) so refreshing=true is set.
    await page.waitForTimeout(50)

    // Fire second gesture immediately while first is still in flight.
    // scrollToBottom + 600 ms still < 800 ms total, so refresh is still running.
    await scrollToBottom(page)
    await page.waitForTimeout(100)  // short — refresh is still running
    await swipe(page, '.thread-body', { dy: -120 })

    // Wait for both to settle (> 800 ms total from first swipe).
    await page.waitForTimeout(900)
    // Only one extra reload should have been triggered.
    expect(reloadCount - countBefore).toBe(1)
  })

  test('pull-refresh panel stays visible in viewport while refreshing', async ({ page }) => {
    // Regression: previously onEnd reset pullPx to 0 before refreshing completed,
    // causing translateY(100%) while refreshing=true → panel fully hidden below viewport.
    // Fix: transform uses translateY(0) while refreshing regardless of pullPx.

    // Make reload slow enough (600 ms) so we can measure panel position mid-flight.
    await mockRoutes(page, {
      reloadHandler: async (route) => {
        await new Promise((r) => setTimeout(r, 600))
        route.fulfill({ json: { res_count: 40, read_res: 0, status: 'active' } })
      },
    })
    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    await scrollToBottom(page)
    // Wait for the 0.5 s lock to expire.
    await page.waitForTimeout(600)

    // Trigger pull-to-refresh gesture (dy=-120 > PULL_THRESHOLD_PX=80).
    await swipe(page, '.thread-body', { dy: -120 })

    // Wait briefly so refreshing=true is set but the slow reload has not finished.
    await page.waitForTimeout(100)

    // Panel must be visible inside the viewport while refresh is in flight.
    const panel = page.getByTestId('pull-refresh')
    const box = await panel.boundingBox()
    const innerHeight = await page.evaluate(() => window.innerHeight)
    // boundingBox.y must be less than innerHeight (panel is inside the viewport).
    // Playwright boundingBox() returns { x, y, width, height } — use .y (top edge).
    expect(box).not.toBeNull()
    expect(box.y).toBeLessThan(innerHeight)

    // After reload finishes, the panel should fold back (y >= innerHeight or off-screen).
    await page.waitForTimeout(700)
    const boxAfter = await panel.boundingBox()
    // After completion, panel is hidden (below viewport) — y should be >= innerHeight.
    if (boxAfter) {
      expect(boxAfter.y).toBeGreaterThanOrEqual(innerHeight)
    }
  })

  test('after pull-to-refresh: old reses have no .unread bar, new reses do', async ({ page }) => {
    // Initial dat: 40 reses, all unread (read_res=0). Must be long enough to scroll.
    const INITIAL_COUNT = 40
    const ADDED_COUNT = 2 // 2 new reses added after refresh

    let reloadCallCount = 0
    const initialDat = {
      title: FAV.title,
      res_count: INITIAL_COUNT,
      read_res: 0,
      status: 'active',
      res: Array.from({ length: INITIAL_COUNT }, (_, i) => ({
        num: i + 1,
        name: '名無し',
        mail: '',
        date: `2025 ID:x${i}`,
        body: `本文${i + 1}`,
      })),
    }
    const refreshedDat = {
      title: FAV.title,
      res_count: INITIAL_COUNT + ADDED_COUNT,
      read_res: 0,
      status: 'active',
      res: Array.from({ length: INITIAL_COUNT + ADDED_COUNT }, (_, i) => ({
        num: i + 1,
        name: '名無し',
        mail: '',
        date: `2025 ID:x${i}`,
        body: `本文${i + 1}`,
      })),
    }

    page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    // First dat call returns initial dat; after reload it returns refreshed dat.
    let datCallCount = 0
    page.route(/\/api\/favorites\/.+\/dat$/, (route) => {
      datCallCount++
      route.fulfill({ json: datCallCount === 1 ? initialDat : refreshedDat })
    })
    page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
      reloadCallCount++
      route.fulfill({ json: { res_count: INITIAL_COUNT + ADDED_COUNT, read_res: 0, status: 'active' } })
    })
    page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: { read_res: 0 } })
      } else {
        route.fulfill({ json: { ok: true } })
      }
    })

    await page.goto(THREAD_PATH)
    // Wait for initial reses to render.
    await expect(page.getByText(`本文${INITIAL_COUNT}`, { exact: true })).toBeVisible()

    // Scroll to the bottom to arm the pull-to-refresh gesture.
    await scrollToBottom(page)
    // Wait for the 0.5 s unlock.
    await page.waitForTimeout(600)

    const reloadsBefore = reloadCallCount

    // Trigger pull-to-refresh.
    await swipe(page, '.thread-body', { dy: -120 })

    // Wait for the refresh to complete (new reses appear).
    await expect(page.getByText(`本文${INITIAL_COUNT + ADDED_COUNT}`, { exact: true })).toBeVisible()
    expect(reloadCallCount).toBeGreaterThan(reloadsBefore)

    // Old reses (num 1..INITIAL_COUNT) must NOT have the .unread class.
    for (let i = 1; i <= INITIAL_COUNT; i++) {
      await expect(page.locator(`.res[data-res="${i}"]`)).not.toHaveClass(/unread/)
    }
    // New reses (num INITIAL_COUNT+1..INITIAL_COUNT+ADDED_COUNT) MUST have .unread.
    for (let i = INITIAL_COUNT + 1; i <= INITIAL_COUNT + ADDED_COUNT; i++) {
      await expect(page.locator(`.res[data-res="${i}"]`)).toHaveClass(/unread/)
    }
  })

  test('opening a thread without scrolling shows unread bar on unread reses (regression)', async ({ page }) => {
    // FAV has read_res=0, so all reses start as unread.
    const COUNT = 5
    page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
      route.fulfill({ json: datResponse(COUNT) }),
    )
    page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
      route.fulfill({ json: { res_count: COUNT, read_res: 0, status: 'active' } }),
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

  test('modal open: pull-to-refresh gesture is suppressed', async ({ page }) => {
    let reloadCount = 0
    const dat = {
      title: FAV.title,
      res_count: 40,
      read_res: 0,
      status: 'active',
      res: [
        ...Array.from({ length: 38 }, (_, i) => ({
          num: i + 1,
          name: '名無し',
          mail: '',
          date: `2025 ID:x${i}`,
          body: `本文${i + 1}`,
        })),
        { num: 39, name: '名無し', mail: '', date: '2025 ID:xa', body: '&gt;&gt;1 アンカー' },
        { num: 40, name: '名無し', mail: '', date: '2025 ID:xb', body: '本文40' },
      ],
    }
    page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
    page.route('**/api/favorites/refresh', (route) =>
      route.fulfill({ json: { ok: true, boards: 0 } }),
    )
    page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: dat }))
    page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
      reloadCount++
      route.fulfill({ json: { res_count: 40, read_res: 0, status: 'active' } })
    })

    await page.goto(THREAD_PATH)
    await expect(page.getByText('本文1', { exact: true })).toBeVisible()

    // Open the anchor modal.
    await page.locator('.anchor[data-anchor="1"]').first().click()
    await expect(page.locator('.modal')).toBeVisible()

    await scrollToBottom(page)
    await page.waitForTimeout(600)

    const countBefore = reloadCount

    // Swipe up with modal open — should be suppressed.
    await swipe(page, '.thread-body', { dy: -120 })
    await page.waitForTimeout(200)
    expect(reloadCount).toBe(countBefore)
  })
})
