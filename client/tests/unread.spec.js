import { test, expect } from '@playwright/test'

// Acceptance cases for the reading rules in docs/spec.md §8.1. A small in-memory
// server stands in for the Rust API: progress POSTs are kept with the same MAX
// guard as the real server, and the list / dat responses report that value, so
// a page reload shows exactly what reached the server.

const THREAD = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '未読スレ',
  rating: 0,
  status: 'active',
}
const OTHER = { ...THREAD, thread_id: '1771127146', title: '別のスレ' }

function post(num, own = false) {
  // Varying heights so positions are not a uniform grid.
  return {
    num,
    name: '名無し',
    mail: '',
    date: `2025/01/01 00:00 ID:x${num}`,
    body: Array.from({ length: (num % 3) + 1 }, () => `本文${num}`).join('<br>'),
    id: `x${num}`,
    own,
  }
}

function threadState(meta, count, readRes) {
  return { meta, res: Array.from({ length: count }, (_, i) => post(i + 1)), read_res: readRes }
}

async function serve(page, threads) {
  const progress = []
  const byId = new Map(threads.map((t) => [t.meta.thread_id, t]))
  const find = (url) => byId.get(url.match(/\/api\/favorites\/[^/]+\/[^/]+\/([^/]+)\//)[1])
  const row = (t) => ({
    ...t.meta,
    res_count: t.res.length,
    read_res: t.read_res,
    own_res: t.res.filter((r) => r.own).map((r) => r.num),
  })

  await page.route('**/api/favorites', (route) => route.fulfill({ json: threads.map(row) }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => {
    const t = find(route.request().url())
    route.fulfill({
      json: {
        title: t.meta.title,
        res_count: t.res.length,
        read_res: t.read_res,
        status: 'active',
        mosaic_urls: [],
        res: t.res,
      },
    })
  })
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) => {
    const t = find(route.request().url())
    t.res.push(...(t.arrivals ?? []))
    t.arrivals = []
    route.fulfill({ json: { res_count: t.res.length, read_res: t.read_res, status: 'active' } })
  })
  await page.route(/\/api\/favorites\/.+\/post$/, (route) => {
    const t = find(route.request().url())
    t.arrivals = [...(t.arrivals ?? []), post(t.res.length + (t.arrivals?.length ?? 0) + 1, true)]
    route.fulfill({ json: { ok: true } })
  })
  await page.route(/\/api\/favorites\/.+\/progress$/, (route) => {
    const t = find(route.request().url())
    if (route.request().method() === 'POST') {
      const n = route.request().postDataJSON().read_res
      progress.push({ thread: t.meta.thread_id, read_res: n })
      t.read_res = Math.max(t.read_res, n)
    }
    route.fulfill({ json: { ok: true } })
  })
  await page.route('**/api/ng-ids', (route) => route.fulfill({ json: [] }))
  await page.route(/\/api\/ng-words(\?|$)/, (route) => route.fulfill({ json: [] }))
  await page.route('**/api/ng-wacchoi', (route) => route.fulfill({ json: [] }))
  return { progress }
}

const badge = (page, title) =>
  page.locator('.list-pane .thread', { hasText: title }).locator('.unread')

async function openFromList(page, title) {
  await page.locator('.list-pane .thread', { hasText: title }).click()
  await expect(page.getByTestId('read-boundary')).toBeAttached()
  // Let the entry positioning finish and the observer report what is on screen.
  await expect(page.locator('.thread-body > .res').first()).toBeVisible()
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))))
}

async function swipeBack(page) {
  await page.locator('.thread-body').evaluate((body) => {
    const touch = (x) =>
      new Touch({ identifier: 1, target: body, clientX: x, clientY: 300, pageX: x, pageY: 300 })
    const fire = (type, x) =>
      body.dispatchEvent(
        new TouchEvent(type, {
          bubbles: true,
          touches: type === 'touchend' ? [] : [touch(x)],
          changedTouches: [touch(x)],
        }),
      )
    fire('touchstart', 20)
    fire('touchmove', 60)
    fire('touchmove', 140)
    fire('touchend', 160)
  })
}

// The rendered posts that reach the scroller viewport, newest first.
async function onScreen(page) {
  return page.locator('.thread-body').evaluate((body) => {
    const view = body.getBoundingClientRect()
    return [...body.querySelectorAll(':scope > .res')]
      .map((n) => ({ num: Number(n.dataset.res), rect: n.getBoundingClientRect() }))
      .filter(({ rect }) => rect.bottom > view.top && rect.top < view.bottom)
      .map(({ num, rect }) => ({ num, top: rect.top - view.top }))
  })
}

async function topOf(page, num) {
  return page.locator(`.thread-body > .res[data-res="${num}"]`).evaluate((n) => {
    const body = n.closest('.thread-body')
    return n.getBoundingClientRect().top - body.getBoundingClientRect().top
  })
}

test.describe('phone: leaving right after a single new post was shown', () => {
  test.use({ viewport: { width: 390, height: 700 } })

  const leaveBy = {
    'the favorites tab': (page) => page.getByTestId('tab-favorites').click(),
    'the browser back button': (page) => page.goBack(),
    'a back swipe': swipeBack,
  }

  for (const [how, leave] of Object.entries(leaveBy)) {
    test(`saves the read post when leaving by ${how}`, async ({ page }) => {
      const thread = threadState(THREAD, 31, 30)
      const server = await serve(page, [thread])
      await page.goto('/')
      await expect(badge(page, THREAD.title)).toHaveText('1')

      await openFromList(page, THREAD.title)
      await leave(page)
      await expect(page.locator('.list-pane .thread', { hasText: THREAD.title })).toBeVisible()
      await expect.poll(() => thread.read_res).toBe(31)

      await page.reload()
      await expect(page.locator('.list-pane .thread', { hasText: THREAD.title })).toBeVisible()
      await expect(badge(page, THREAD.title)).toHaveCount(0)
      expect(server.progress.at(-1)).toEqual({ thread: THREAD.thread_id, read_res: 31 })
    })
  }
})

test.describe('PC: switching threads in the two-column layout', () => {
  test.use({ viewport: { width: 1200, height: 800 } })

  test('saves the previous thread’s read position', async ({ page }) => {
    const first = threadState(THREAD, 31, 30)
    const second = threadState(OTHER, 10, 10)
    await serve(page, [first, second])
    await page.goto('/')

    await openFromList(page, THREAD.title)
    await page.locator('.list-pane .thread', { hasText: OTHER.title }).click()
    await expect(page.getByTestId('thread-title')).toHaveText(OTHER.title)
    await expect.poll(() => first.read_res).toBe(31)
  })
})

test.describe('phone: posting or refreshing with more than ten unread posts', () => {
  test.use({ viewport: { width: 390, height: 700 } })

  // Where the reader is when they post: inside the unread section, below the
  // previous-read marker among older posts, and at the very top (all read).
  const positions = {
    'inside the unread section': async (body) => body.evaluate((b) => b.scrollBy(0, -250)),
    'among older posts below the marker': async (body) => body.evaluate((b) => b.scrollBy(0, 2500)),
    'at the top': async (body) => body.evaluate((b) => (b.scrollTop = 0)),
  }

  const actions = {
    posting: async (page) => {
      await page.getByRole('button', { name: '書き込む' }).click()
      await page.locator('.post-textarea').fill('テスト')
      await page.locator('.post-submit').click()
      await expect(page.locator('.thread-body > .res.own')).toHaveCount(1)
    },
    'the refresh button': async (page, thread) => {
      thread.arrivals = [post(61)]
      await page.locator('.detail-pane').getByRole('button', { name: '更新' }).click()
      await expect(page.locator('.thread-body > .res[data-res="61"]')).toBeAttached()
    },
  }

  for (const [where, scrollTo] of Object.entries(positions)) {
    for (const [what, act] of Object.entries(actions)) {
      test(`${what} keeps the screen still when ${where}`, async ({ page }) => {
        const thread = threadState(THREAD, 60, 40)
        const server = await serve(page, [thread])
        await page.goto('/')
        await openFromList(page, THREAD.title)
        // Let the idle batches render every older post before moving around.
        await expect(page.locator('.thread-body > .res')).toHaveCount(60)
        const shownAtEntry = await onScreen(page)

        const body = page.locator('.thread-body')
        await scrollTo(body)
        await page.evaluate(
          () => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))),
        )
        const before = await onScreen(page)
        const anchor = before[0]
        const unreadBefore = await page.locator('.thread-body > .res.unread').count()

        await act(page, thread)
        await page.evaluate(
          () => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 50))),
        )

        // The post that was at the top of the screen did not move.
        expect(Math.abs((await topOf(page, anchor.num)) - anchor.top)).toBeLessThanOrEqual(1)
        // The new post is added at the top of the timeline.
        await expect(page.locator('.thread-body > .res').first()).toHaveAttribute('data-res', '61')
        if (what === 'posting') {
          await expect(page.locator('.thread-body > .res').first()).toHaveClass(/\bown\b/)
          await expect(page.locator('.thread-body > .res.unread')).toHaveCount(unreadBefore)
        } else {
          await expect(page.locator('.thread-body > .res').first()).toHaveClass(/\bunread\b/)
          await expect(page.locator('.thread-body > .res.unread')).toHaveCount(unreadBefore + 1)
        }

        // Leave and check the saved position: only posts that reached the screen
        // count, so posts added above it are not read by the action itself.
        const shown = [...shownAtEntry, ...before, ...(await onScreen(page))].map((r) => r.num)
        await page.getByTestId('tab-favorites').click()
        await expect.poll(() => server.progress.length).toBeGreaterThan(0)
        expect(thread.read_res).toBe(Math.max(...shown))
        await expect(page.locator('.list-pane .thread', { hasText: THREAD.title })).toContainText(
          'アプリ 61',
        )
      })
    }
  }
})

test.describe('list: own posts are not unread', () => {
  test('the badge counts only other people’s posts after the read position', async ({ page }) => {
    const ownOnly = threadState(THREAD, 60, 60)
    ownOnly.res.push(post(61, true))
    const mixed = threadState(OTHER, 60, 50)
    mixed.res.push(post(61, true), post(62))
    await serve(page, [ownOnly, mixed])
    await page.goto('/')

    await expect(page.locator('.list-pane .thread', { hasText: THREAD.title })).toBeVisible()
    await expect(badge(page, THREAD.title)).toHaveCount(0)
    await expect(badge(page, OTHER.title)).toHaveText('11')
  })
})

test.describe('phone: returning to the list after posting', () => {
  test.use({ viewport: { width: 390, height: 700 } })

  for (const [how, leave] of Object.entries({
    'the favorites tab': (page) => page.getByTestId('tab-favorites').click(),
    'the browser back button': (page) => page.goBack(),
    'a back swipe': swipeBack,
  })) {
    test(`shows the new post count when returning by ${how}`, async ({ page }) => {
      const thread = threadState(THREAD, 20, 20)
      await serve(page, [thread])
      await page.goto('/')
      await openFromList(page, THREAD.title)
      await page.getByRole('button', { name: '書き込む' }).click()
      await page.locator('.post-textarea').fill('テスト')
      await page.locator('.post-submit').click()
      await expect(page.locator('.thread-body > .res.own')).toHaveCount(1)

      await leave(page)
      await expect(page.locator('.list-pane .thread', { hasText: THREAD.title })).toContainText(
        'アプリ 21',
      )
      await expect(badge(page, THREAD.title)).toHaveCount(0)
    })
  }
})
