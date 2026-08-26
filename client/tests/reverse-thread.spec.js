import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '逆順テストスレ',
  res_count: 120,
  read_res: 100,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

function datResponse(fav = FAV) {
  return {
    title: fav.title,
    res_count: fav.res_count,
    read_res: fav.read_res,
    status: fav.status,
    mosaic_urls: [],
    res: Array.from({ length: fav.res_count }, (_, i) => ({
      num: i + 1,
      name: '名無し',
      mail: '',
      date: `2025/01/01 00:00 ID:x${i + 1}`,
      body: `本文${i + 1}`,
      id: `x${i + 1}`,
      own: false,
    })),
  }
}

async function setup(page, fav = FAV) {
  // Hold requestIdleCallback jobs so the test can observe the first painted batch,
  // then release older batches one at a time.
  await page.addInitScript(() => {
    const jobs = new Map()
    let nextId = 1
    window.requestIdleCallback = (callback) => {
      const id = nextId++
      jobs.set(id, callback)
      return id
    }
    window.cancelIdleCallback = (id) => jobs.delete(id)
    window.__runNextIdleJob = () => {
      const next = jobs.entries().next()
      if (next.done) return false
      const [id, callback] = next.value
      jobs.delete(id)
      callback({ didTimeout: false, timeRemaining: () => 50 })
      return true
    }
  })

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [fav] }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(fav) }),
  )
  await page.route(/\/api\/favorites\/.+\/progress$/, (route) =>
    route.fulfill({ json: { ok: true } }),
  )
  await page.route('**/api/ng-ids', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-words(\?|$)/, (route) => route.fulfill({ json: [] }))
  await page.route('**/api/ng-wacchoi', (route) => route.fulfill({ json: [] }))
}

test('newest posts render first and older posts append below without moving the read boundary', async ({
  page,
}) => {
  await setup(page)
  await page.goto(THREAD_PATH)

  const posts = page.locator('.thread-body > .res')
  await expect(posts).toHaveCount(21)
  await expect(posts.first()).toHaveAttribute('data-res', '120')
  await expect(posts.last()).toHaveAttribute('data-res', '100')
  await expect(page.getByTestId('thread-end')).toHaveText('おわり')
  await expect(page.getByTestId('thread-end').locator('+ .res')).toHaveAttribute('data-res', '120')
  await expect(page.getByText('本文99', { exact: true })).toHaveCount(0)
  // The brief pre-position paint at the top must not mark the latest post read.
  // Only posts actually visible after the boundary is positioned may advance progress.
  const unreadBadge = page.locator('.list-pane .unread')
  await expect(unreadBadge).toBeVisible()
  expect(Number(await unreadBadge.textContent())).toBeGreaterThan(0)

  const boundary = page.getByTestId('read-boundary')
  await expect(boundary).toHaveText('前回ここまで')
  await expect(boundary.locator('+ .res')).toHaveAttribute('data-res', '100')
  const before = await page.locator('.thread-body').evaluate((body) => {
    const marker = body.querySelector('[data-testid="read-boundary"]')
    const bodyRect = body.getBoundingClientRect()
    const markerRect = marker.getBoundingClientRect()
    return {
      markerTop: markerRect.top,
      bottomGap: bodyRect.bottom - markerRect.bottom,
    }
  })
  expect(Math.abs(before.bottomGap)).toBeLessThanOrEqual(1)

  expect(await page.evaluate(() => window.__runNextIdleJob())).toBe(true)
  await expect(posts).toHaveCount(71)
  await expect(page.getByText('本文99', { exact: true })).toBeAttached()
  await expect(page.getByText('本文50', { exact: true })).toBeAttached()
  await expect(page.getByText('本文49', { exact: true })).toHaveCount(0)

  const after = await boundary.evaluate((marker) => marker.getBoundingClientRect().top)
  expect(Math.abs(after - before.markerTop)).toBeLessThanOrEqual(1)

  expect(await page.evaluate(() => window.__runNextIdleJob())).toBe(true)
  await expect(posts).toHaveCount(120)
  await expect(posts.last()).toHaveAttribute('data-res', '1')
})

test('a 590-post thread with no unread posts places the boundary above res 590', async ({
  page,
}) => {
  const fullyReadFav = { ...FAV, res_count: 590, read_res: 590 }
  await setup(page, fullyReadFav)
  await page.goto(THREAD_PATH)

  const boundary = page.getByTestId('read-boundary')
  const posts = page.locator('.thread-body > .res')
  // One urgent post cannot fill the viewport. Older posts should be rendered
  // naturally below it instead of inserting a synthetic top spacer.
  await expect(posts).toHaveCount(51)
  await expect(page.locator('.entry-spacer')).toHaveCount(0)
  await expect(boundary.locator('+ .res')).toHaveAttribute('data-res', '590')
  await expect(posts.nth(1)).toHaveAttribute('data-res', '589')
  await expect(page.locator('.thread-body')).toHaveJSProperty('scrollTop', 0)
})

test('an entirely unread thread keeps one boundary after res 1', async ({ page }) => {
  await setup(page, { ...FAV, read_res: 0 })
  await page.goto(THREAD_PATH)

  const posts = page.locator('.thread-body > .res')
  await expect(posts).toHaveCount(120)
  await expect(posts.first()).toHaveAttribute('data-res', '120')
  await expect(posts.last()).toHaveAttribute('data-res', '1')
  await expect(page.getByTestId('read-boundary')).toHaveCount(1)
  await expect(posts.last().locator('+ [data-testid="read-boundary"]')).toHaveCount(1)
  await expect(page.getByTestId('thread-end')).toHaveText('おわり')
})

test('progress tracking starts when an initially empty thread gains a post', async ({ page }) => {
  const emptyFav = { ...FAV, res_count: 0, read_res: 0 }
  let count = 0
  let savedProgress = null

  await setup(page, emptyFav)
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse({ ...emptyFav, res_count: count }) }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
    count = 1
    route.fulfill({ json: { res_count: 1, read_res: 0, status: 'active' } })
  })
  await page.route(/\/api\/favorites\/.+\/progress$/, async (route) => {
    savedProgress = route.request().postDataJSON().read_res
    await route.fulfill({ json: { ok: true } })
  })

  await page.goto(THREAD_PATH)
  await expect(page.locator('.thread-body > .res')).toHaveCount(0)
  // Let the empty-entry restoration frame enable tracking before refreshing.
  await page.evaluate(() => new Promise(requestAnimationFrame))

  await page.locator('.detail-pane').getByRole('button', { name: '更新' }).click()
  await expect(page.locator('.thread-body > .res')).toHaveCount(1)
  await expect.poll(() => savedProgress, { timeout: 4000 }).toBe(1)
})

test('switching threads cancels the previous view restoration frames', async ({ page }) => {
  const secondFav = {
    ...FAV,
    thread_id: '1771127146',
    title: '切替先スレ',
  }
  await setup(page)
  await page.addInitScript(() => {
    const frames = new Map()
    let nextFrameId = 1
    window.requestAnimationFrame = (callback) => {
      const id = nextFrameId++
      frames.set(id, callback)
      return id
    }
    window.cancelAnimationFrame = (id) => frames.delete(id)
    window.__pendingFrames = () => frames.size
    window.__runAllFrames = () => {
      while (frames.size > 0) {
        const jobs = [...frames.entries()]
        frames.clear()
        for (const [, callback] of jobs) callback(performance.now())
      }
    }
  })
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV, secondFav] }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => {
    const fav = route.request().url().includes(secondFav.thread_id) ? secondFav : FAV
    route.fulfill({ json: datResponse(fav) })
  })

  await page.goto('/')
  await page.locator('.info').first().click()
  await expect(page.getByTestId('read-boundary')).toBeAttached()
  expect(await page.evaluate(() => window.__pendingFrames())).toBe(1)

  await page.locator('.info').nth(1).click()
  await expect(page.getByTestId('thread-title')).toHaveText(secondFav.title)
  await expect(page.getByTestId('read-boundary')).toBeAttached()
  // The keyed remount must cancel the old view's queued restore frame. Only
  // the new view's first positioning frame may remain.
  expect(await page.evaluate(() => window.__pendingFrames())).toBe(1)
  await page.evaluate(() => window.__runAllFrames())
  await expect(page.getByTestId('thread-title')).toHaveText(secondFav.title)
  expect(await page.evaluate(() => window.__pendingFrames())).toBe(0)
})
