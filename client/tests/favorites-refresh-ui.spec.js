import { test, expect } from '@playwright/test'

const FAVS = [
  {
    server: 'egg',
    board: 'applism',
    board_name: 'アプリ',
    thread_id: '1771127001',
    title: 'テストスレ1',
    res_count: 10,
    read_res: 5,
    rating: 3,
    status: 'active',
  },
]

function mockBase(page, { refreshDelay = 0 } = {}) {
  page.route('**/api/favorites/refresh', async (route) => {
    if (refreshDelay > 0) await new Promise((r) => setTimeout(r, refreshDelay))
    route.fulfill({ json: { ok: true, boards: 0 } })
  })
}

// ─── Footer refresh button tests ────────────────────────────────────────────

test('footer refresh button is visible on the favorites list', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await mockBase(page)
  await page.goto('/')

  const btn = page.getByTestId('favorites-refresh-btn')
  await expect(btn).toBeVisible()
  await expect(btn).toContainText('更新')
})

test('pressing the refresh button calls POST /api/favorites/refresh', async ({ page }) => {
  let refreshCount = 0
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => {
    refreshCount++
    route.fulfill({ json: { ok: true, boards: 0 } })
  })
  await page.goto('/')

  // Wait for the initial auto-refresh (fired on mount) to complete.
  await expect.poll(() => refreshCount).toBeGreaterThanOrEqual(1)
  const countBefore = refreshCount

  await page.getByTestId('favorites-refresh-btn').click()
  await expect.poll(() => refreshCount).toBeGreaterThan(countBefore)
})

test('refresh button is disabled while refreshing', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  // Make refresh slow so we can observe the disabled state mid-flight.
  // Total time: 200ms (API delay) + 1500ms (REFRESH_RELIST_DELAY_MS) + load() + 1800ms = ~3.5s.
  await mockBase(page, { refreshDelay: 200 })
  await page.goto('/')

  const btn = page.getByTestId('favorites-refresh-btn')
  await expect(btn).toBeVisible()

  // Wait for auto-refresh on mount to complete so it doesn't interfere.
  await expect(btn).toBeEnabled({ timeout: 6000 })

  // Click — button should become disabled immediately.
  await btn.click()
  await expect(btn).toBeDisabled()

  // After the full refresh cycle (API + 1.5s relist delay + 1.8s buffer = ~3.5s total),
  // the button re-enables. Use a generous timeout.
  await expect(btn).toBeEnabled({ timeout: 6000 })
})

test('after refresh, GET /api/favorites is called again', async ({ page }) => {
  let listCount = 0
  await page.route('**/api/favorites', (route) => {
    listCount++
    route.fulfill({ json: FAVS })
  })
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.goto('/')

  // Wait for initial load + warm refresh on mount.
  await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()
  await page.waitForTimeout(200)
  const countBefore = listCount

  await page.getByTestId('favorites-refresh-btn').click()

  // After the 1.5 s relist delay + 1.8 s refreshing state buffer, the list
  // should have been re-fetched. Use a generous timeout.
  await expect.poll(() => listCount, { timeout: 5000 }).toBeGreaterThan(countBefore)
})

// ─── Top pull-to-refresh panel (mobile) ─────────────────────────────────────

test.describe('top pull-to-refresh (touch, mobile)', () => {
  test.use({ viewport: { width: 390, height: 700 }, hasTouch: true })

  // Dispatch a full touchstart → touchmove → touchend sequence on .favorites-body.
  // The topPullRefresh action attaches its listeners to this node.
  // startY / endY are clientY values (0 = top of viewport).
  async function dispatchPullGesture(page, { startY, endY }) {
    await page.evaluate(
      ({ startY, endY }) => {
        const node = document.querySelector('.favorites-body')
        const x = 100
        node.dispatchEvent(
          new TouchEvent('touchstart', {
            bubbles: true,
            touches: [new Touch({ identifier: 1, target: node, clientX: x, clientY: startY })],
          }),
        )
        node.dispatchEvent(
          new TouchEvent('touchmove', {
            bubbles: true,
            touches: [new Touch({ identifier: 1, target: node, clientX: x, clientY: endY })],
          }),
        )
        node.dispatchEvent(
          new TouchEvent('touchend', {
            bubbles: true,
            changedTouches: [new Touch({ identifier: 1, target: node, clientX: x, clientY: endY })],
          }),
        )
      },
      { startY, endY },
    )
  }

  test('pull-refresh-top panel is present in the DOM', async ({ page }) => {
    await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
    await mockBase(page)
    await page.goto('/')

    const panel = page.getByTestId('pull-refresh-top')
    await expect(panel).toBeAttached()
  })

  test('downward drag from top shows the pull-refresh panel', async ({ page }) => {
    await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
    await mockBase(page)
    await page.goto('/')

    // Wait for initial content.
    await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()

    // Simulate a downward drag from the very top of the page using touch events.
    await page.evaluate(() => {
      // Ensure scrollTop is at 0 before the gesture.
      document.scrollingElement.scrollTop = 0

      const node = document.querySelector('.favorites-body')
      const x0 = window.innerWidth / 2
      const y0 = 100 // start near the top

      function mk(type, x, y, active) {
        const t = new Touch({ identifier: 1, target: node, clientX: x, clientY: y })
        node.dispatchEvent(
          new TouchEvent(type, {
            bubbles: true,
            cancelable: true,
            touches: active ? [t] : [],
            targetTouches: active ? [t] : [],
            changedTouches: [t],
          }),
        )
      }

      // Drag downward 120 px (> PULL_THRESHOLD_PX=80) but do NOT release yet.
      mk('touchstart', x0, y0, true)
      mk('touchmove', x0, y0 + 40, true)
      mk('touchmove', x0, y0 + 90, true)
    })

    // The panel height should grow above 0.
    const panel = page.getByTestId('pull-refresh-top')
    const height = await panel.evaluate((el) => el.getBoundingClientRect().height)
    expect(height).toBeGreaterThan(0)

    // Cleanup: fire touchend to reset.
    await page.evaluate(() => {
      const node = document.querySelector('.favorites-body')
      const x0 = window.innerWidth / 2
      const t = new Touch({ identifier: 1, target: node, clientX: x0, clientY: 190 })
      node.dispatchEvent(
        new TouchEvent('touchend', {
          bubbles: true,
          cancelable: true,
          touches: [],
          targetTouches: [],
          changedTouches: [t],
        }),
      )
    })
  })

  test('release past threshold (>80px) triggers POST /api/favorites/refresh', async ({ page }) => {
    let refreshCount = 0
    await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
    await page.route('**/api/favorites/refresh', (route) => {
      refreshCount++
      route.fulfill({ json: { ok: true, boards: 0 } })
    })
    await page.goto('/')

    // Wait for the initial auto-refresh on mount to complete.
    await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()
    await expect.poll(() => refreshCount).toBeGreaterThanOrEqual(1)
    const before = refreshCount

    // Drag 130px down from clientY=10 (past PULL_THRESHOLD_PX=80) and release.
    await dispatchPullGesture(page, { startY: 10, endY: 140 })

    await expect.poll(() => refreshCount, { timeout: 4000 }).toBeGreaterThan(before)
  })

  test('release below threshold (<80px) does NOT trigger refresh', async ({ page }) => {
    let refreshCount = 0
    await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
    await page.route('**/api/favorites/refresh', (route) => {
      refreshCount++
      route.fulfill({ json: { ok: true, boards: 0 } })
    })
    await page.goto('/')

    // Wait for the initial auto-refresh on mount to complete.
    await expect(page.getByTestId('favorites-refresh-btn')).toBeVisible()
    await expect.poll(() => refreshCount).toBeGreaterThanOrEqual(1)
    const before = refreshCount

    // Drag only 50px down from clientY=10 (below PULL_THRESHOLD_PX=80) and release.
    await dispatchPullGesture(page, { startY: 10, endY: 60 })

    // Wait 200ms to ensure no refresh fires.
    await page.waitForTimeout(200)
    expect(refreshCount).toBe(before)
  })
})
