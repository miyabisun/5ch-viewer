// Integration tests: status boundary behavior across the HEAD-gated reload.
//
// compute_status derives status purely from res_count (not dat byte size).
// These tests verify the warned/dead thresholds at the RES_WARN (980) and
// RES_DEAD (1002) boundaries through the real reload endpoint.

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

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

// Scenario: compute_status uses only res_count -- NOT dat byte size.
// A thread with RES_WARN (980) posts is 'warned'; changing byte size does not affect that.
test('status is warned at res_count=980 and stays warned after re-fetch', async ({ request }) => {
  // Seed with 980 posts (warned threshold).
  await seedFavorite(request, { res_count: 980, blob_posts: 980 })
  // Subject reports growth (981 > 980); dat returns 981 posts (still warned).
  await programMockThread(request, { res_count: 981, dat_posts: 981 })

  const reload = await reloadThread(request)
  expect(reload.updated).toBe(true)
  // 981 posts → still warned (< 1002).
  expect(reload.status).toBe('warned')
})

// Scenario: compute_status marks warned at res_count=1000 (RES_DEAD=1002; 1000/1001 are warned).
// (Previously this test expected 'dead', which was wrong after sentinel alignment in d69e853.)
test('status is warned at res_count=1000 (byte size does not matter)', async ({ request }) => {
  await seedFavorite(request, { res_count: 999, blob_posts: 999 })
  // Mock returns 1000 posts → Content-Length larger than 999-post seed.
  await programMockThread(request, { res_count: 1000, dat_posts: 1000 })

  const reload = await reloadThread(request)
  expect(reload.updated).toBe(true)
  expect(reload.status).toBe('warned')
})
