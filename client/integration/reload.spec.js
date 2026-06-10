import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL, MOCK_URL } from '../playwright.integration.config.js'

const FAV = {
  server: 'egg',
  board: 'applism',
  thread_id: '1771127145',
  title: 'テストスレ',
}

// Helpers hit the test-only control endpoints directly (not through the /api proxy).
async function reset(ctx) {
  const r = await ctx.post(`${APP_URL}/_control/reset`)
  expect(r.ok()).toBeTruthy()
}
async function seedFavorite(ctx, { res_count, blob_posts }) {
  const r = await ctx.post(`${APP_URL}/_control/seed-favorite`, {
    data: { ...FAV, res_count, blob_posts },
  })
  expect(r.ok()).toBeTruthy()
}
async function programMockThread(ctx, { res_count, dat_posts, gone = false }) {
  const r = await ctx.post(`${MOCK_URL}/_control/thread`, {
    data: { ...FAV, res_count, dat_posts, gone },
  })
  expect(r.ok()).toBeTruthy()
}

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

// Regression (the "stuck at 111" bug), proven through the REAL backend:
// the favorite's metadata res_count has drifted to 117 while the stored dat blob holds
// only 111 posts. The mock 5ch reports subject=117 and serves a 117-post dat. Opening the
// thread runs the viewer reload (GET): the gate keys on the blob count (111 < 117), so the
// dat is fetched, the blob is fully replaced, and the view renders all 117 posts.
test('reload heals a drifted favorite (meta 117 / blob 111 -> 117 posts shown)', async ({
  page,
  request,
}) => {
  // Drifted state: metadata says 117, but the actual stored dat is only 111 posts.
  await seedFavorite(request, { res_count: 117, blob_posts: 111 })
  // 5ch now has 117 posts (subject + dat agree).
  await programMockThread(request, { res_count: 117, dat_posts: 117 })

  await page.goto('/')
  // Open the thread (auto-refresh: GET reload, then render the grown dat).
  await page.locator('.info').first().click()

  // The latest post (117) is rendered through the real fetch->DB-replace->getDat flow.
  await expect(page.getByText('本文117', { exact: true })).toBeVisible()
  // Sanity: post 112 (beyond the stale 111 ceiling) is also present.
  await expect(page.getByText('本文112', { exact: true })).toBeVisible()
})

// Load-reduction gate, end to end: when subject reports no growth (matches the stored
// blob), the dat is NOT re-fetched and the existing posts render unchanged.
test('reload skips the dat fetch when subject shows no growth', async ({
  page,
  request,
}) => {
  await seedFavorite(request, { res_count: 5, blob_posts: 5 })
  // Mock would serve 999 posts IF asked — but subject=5 matches the blob, so it is not asked.
  await programMockThread(request, { res_count: 5, dat_posts: 999 })

  await page.goto('/')
  await page.locator('.info').first().click()

  await expect(page.getByText('本文5', { exact: true })).toBeVisible()
  // The skip path means the 999-post body was never fetched.
  await expect(page.getByText('本文6', { exact: true })).toHaveCount(0)
})
