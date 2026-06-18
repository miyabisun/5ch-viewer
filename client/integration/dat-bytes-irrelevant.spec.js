// Integration tests: dat byte size must NOT affect the fetch gate or thread status.
// Status (active/warned/dead) is now derived from res_count alone. A dat that changes
// in byte size without changing its res_count must not trigger a new fetch and must not
// alter the stored status.

import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL, MOCK_URL } from '../playwright.integration.config.js'

const FAV = {
  server: 'egg',
  board: 'datbytes',
  thread_id: '1771200000',
  title: 'バイトサイズ検証スレ',
}

async function reset(ctx) {
  expect((await ctx.post(`${APP_URL}/_control/reset`)).ok()).toBeTruthy()
  expect((await ctx.post(`${MOCK_URL}/_control/reset`)).ok()).toBeTruthy()
}

async function seedFavorite(ctx, opts) {
  const r = await ctx.post(`${APP_URL}/_control/seed-favorite`, {
    data: { ...FAV, ...opts },
  })
  expect(r.ok()).toBeTruthy()
}

async function programMockThread(ctx, opts) {
  const r = await ctx.post(`${MOCK_URL}/_control/thread`, {
    data: { ...FAV, ...opts },
  })
  expect(r.ok()).toBeTruthy()
}

async function reloadThread(ctx) {
  const r = await ctx.get(
    `${APP_URL}/api/favorites/${FAV.server}/${FAV.board}/${FAV.thread_id}/reload`,
  )
  expect(r.ok()).toBeTruthy()
  return r.json()
}

async function getDat(ctx) {
  const r = await ctx.get(
    `${APP_URL}/api/favorites/${FAV.server}/${FAV.board}/${FAV.thread_id}/dat`,
  )
  expect(r.ok()).toBeTruthy()
  return r.json()
}

async function datHits(ctx) {
  // Count how many times the mock dat endpoint was called for this thread.
  // We use subject-hits as a proxy — but subject is read once per reload call anyway.
  // Instead, we rely on the stored dat post count to prove no new fetch happened.
  return null // (used indirectly via storedPosts comparison)
}

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

// Scenario: res_count unchanged but the mock would return a larger body if asked.
// The gate must NOT fetch (subject res_count == blob count), so storedPosts stays the same.
test('no dat fetch when res_count is unchanged, regardless of potential dat size change', async ({
  request,
}) => {
  // Seed: 10 posts in both blob and metadata.
  await seedFavorite(request, { res_count: 10, blob_posts: 10 })
  // Mock: subject still reports 10, but dat would return 999 posts (huge body) IF fetched.
  await programMockThread(request, { res_count: 10, dat_posts: 999 })

  const before = await getDat(request)
  expect(before.res.length).toBe(10)

  const reload = await reloadThread(request)
  // The gate must skip the fetch (subject 10 == blob 10).
  expect(reload.updated).toBe(false)

  const after = await getDat(request)
  // Stored dat is still 10 posts — the 999-post body was never fetched.
  expect(after.res.length).toBe(10)
})

// Scenario: res_count increases → dat IS fetched regardless of byte size.
// This confirms the gate is purely res_count-driven.
test('dat IS fetched when res_count grows (byte size irrelevant)', async ({ request }) => {
  await seedFavorite(request, { res_count: 10, blob_posts: 10 })
  // subject reports growth (11 > 10); dat returns 11 posts.
  await programMockThread(request, { res_count: 11, dat_posts: 11 })

  const reload = await reloadThread(request)
  expect(reload.updated).toBe(true)

  const after = await getDat(request)
  expect(after.res.length).toBe(11)
})

// Scenario: compute_status uses only res_count — NOT dat byte size.
// A thread with RES_WARN (980) posts is 'warned'; adding more bytes does not change that.
// After a dat fetch that keeps res_count at 980, status must remain 'warned'.
test('status is warned at res_count=980 and stays warned after re-fetch', async ({ request }) => {
  // Seed with 980 posts (warned threshold).
  await seedFavorite(request, { res_count: 980, blob_posts: 980 })
  // Subject reports growth (981 > 980); dat returns 981 posts (still warned).
  await programMockThread(request, { res_count: 981, dat_posts: 981 })

  const reload = await reloadThread(request)
  expect(reload.updated).toBe(true)
  // 981 posts → still warned (< 1000).
  expect(reload.status).toBe('warned')
})

// Scenario: compute_status marks warned at res_count=1000 (RES_DEAD=1002; 1000/1001 are warned).
// (Previously this test expected 'dead', which was wrong after sentinel alignment in d69e853.)
test('status is warned at res_count=1000 (byte size does not matter)', async ({ request }) => {
  await seedFavorite(request, { res_count: 999, blob_posts: 999 })
  // Subject reports 1000; dat returns 1000 posts → warned (dead threshold is 1002).
  await programMockThread(request, { res_count: 1000, dat_posts: 1000 })

  const reload = await reloadThread(request)
  expect(reload.updated).toBe(true)
  expect(reload.status).toBe('warned')
})
