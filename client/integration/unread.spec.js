import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL, MOCK_URL } from '../playwright.integration.config.js'

// The unread rule through the real backend (docs/spec.md §8.1): a post written
// from the viewer is recorded as the user's own, and neither the favorites list
// nor /api/news counts it as unread.

const FAV = {
  server: 'egg',
  board: 'applism',
  thread_id: '1771127145',
  title: '未読テストスレ',
}
const API = `${APP_URL}/api/favorites/${FAV.server}/${FAV.board}/${FAV.thread_id}`

async function ok(response) {
  expect(response.ok(), `${response.url()} → ${response.status()}`).toBeTruthy()
  return response.json()
}

async function mockThread(ctx, count) {
  await ok(
    await ctx.post(`${MOCK_URL}/_control/thread`, {
      data: { ...FAV, res_count: count, dat_posts: count, gone: false },
    }),
  )
}

const newsIds = async (ctx) =>
  (await ok(await ctx.get(`${APP_URL}/api/news`))).items.map((item) => item.id)
const listed = async (ctx) =>
  (await ok(await ctx.get(`${APP_URL}/api/favorites`))).find((f) => f.thread_id === FAV.thread_id)

test('a post from the viewer is not unread in the list or the news feed', async ({ page }) => {
  const ctx = await pwRequest.newContext()
  await ok(await ctx.post(`${APP_URL}/_control/reset`))
  await ok(await ctx.post(`${MOCK_URL}/_control/reset`))
  await ok(
    await ctx.post(`${APP_URL}/_control/seed-favorite`, {
      data: { ...FAV, res_count: 20, blob_posts: 20 },
    }),
  )
  await ok(await ctx.post(`${API}/progress`, { data: { read_res: 20 } }))
  await ok(await ctx.patch(`${API}/rating`, { data: { rating: 3 } }))

  // The user writes res 21; 5ch's dat then holds it.
  await ok(await ctx.post(`${MOCK_URL}/_control/bbs-cgi`, { data: { next_res: 21 } }))
  await mockThread(ctx, 21)
  await ok(await ctx.post(`${API}/post`, { data: { message: 'テスト' } }))
  await ok(await ctx.get(`${API}/reload`))

  const afterPost = await listed(ctx)
  expect(afterPost).toMatchObject({ res_count: 21, read_res: 20, own_res: [21] })
  expect(await newsIds(ctx)).toEqual([])
  await page.goto('/')
  await expect(page.locator('.list-pane .thread', { hasText: FAV.title })).toBeVisible()
  await expect(page.locator('.list-pane .thread .unread')).toHaveCount(0)

  // Someone else replies: only that post is unread.
  await mockThread(ctx, 22)
  await ok(await ctx.get(`${API}/reload`))
  expect(await newsIds(ctx)).toEqual([`${FAV.server}/${FAV.board}/${FAV.thread_id}`])
  const news = await ok(await ctx.get(`${APP_URL}/api/news`))
  expect(news.items[0]._news.unread).toBe(1)
  await page.reload()
  await expect(page.locator('.list-pane .thread .unread')).toHaveText('1')
  await ctx.dispose()
})
