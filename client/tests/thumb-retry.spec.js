import { test, expect } from '@playwright/test'

// Images are prefetched in the background after the dat arrives, so a thumbnail can be
// requested before its image is cached (404). The thumbnail must retry a bounded number
// of times instead of keeping the failure cross until the thread is reopened.

const fav = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '123',
  title: '画像の後追い取得',
  res_count: 1,
  read_res: 0,
  rating: 0,
  status: 'active',
}
const svg =
  '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48"><rect width="48" height="48" fill="#196eaa"/></svg>'

async function mock(page, notFoundCount) {
  let imageRequests = 0
  await page.route('**/api/**', (route) => {
    const url = new URL(route.request().url())
    if (url.pathname === '/api/favorites') return route.fulfill({ json: [fav] })
    if (url.pathname.endsWith('/dat'))
      return route.fulfill({
        json: {
          ...fav,
          mosaic_urls: [],
          res: [
            {
              num: 1,
              name: '名無し',
              mail: '',
              date: '2026/10/06',
              id: null,
              body: '画像<br>https://example.com/late.png',
            },
          ],
        },
      })
    if (url.pathname === '/api/images/example.com/late.png') {
      imageRequests++
      if (imageRequests <= notFoundCount)
        return route.fulfill({
          status: 404,
          headers: { 'cache-control': 'no-store' },
          json: { error: 'image not cached' },
        })
      return route.fulfill({ contentType: 'image/svg+xml', body: svg })
    }
    return route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } })
  })
  return { imageRequests: () => imageRequests }
}

async function open(page) {
  await page.clock.install()
  await page.goto('/')
  await page.locator('.info').filter({ hasText: fav.title }).click()
  return page.locator('.thread-body .thumb')
}

const loaded = (thumb) => thumb.evaluate((img) => img.complete && img.naturalWidth > 0)

test('a thumbnail requested before its prefetch finishes loads once the image is cached', async ({
  page,
}) => {
  const api = await mock(page, 1)
  const thumb = await open(page)
  await expect.poll(api.imageRequests).toBe(1)
  await expect(thumb).not.toHaveClass(/thumb-missing/)

  await page.clock.runFor(30_000)
  await expect.poll(api.imageRequests).toBe(2)
  await expect.poll(() => loaded(thumb)).toBe(true)
  await expect(thumb).not.toHaveClass(/thumb-missing/)
  await expect(page.locator('.thread-body .thumb-error')).toBeHidden()
})

test('a thumbnail that never becomes available shows the cross after a bounded number of retries', async ({
  page,
}) => {
  const api = await mock(page, Infinity)
  const thumb = await open(page)
  await expect.poll(api.imageRequests).toBe(1)

  // Each retry is scheduled only after the previous reload fails, so advance in steps.
  await expect
    .poll(async () => {
      await page.clock.runFor(5_000)
      return thumb.evaluate((img) => img.classList.contains('thumb-missing'))
    })
    .toBe(true)
  await expect(page.locator('.thread-body .thumb-error')).toBeVisible()
  const requests = api.imageRequests()
  expect(requests).toBeGreaterThan(1)

  await page.clock.runFor(600_000)
  expect(api.imageRequests()).toBe(requests)
})
