import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL, MOCK_URL } from '../playwright.integration.config.js'

// All favorites share one board so a single subject.txt read covers them all.
const SERVER = 'egg'
const BOARD = 'applism'
const THREADS = ['1771127001', '1771127002', '1771127003']

async function reset(ctx) {
  expect((await ctx.post(`${APP_URL}/_control/reset`)).ok()).toBeTruthy()
  expect((await ctx.post(`${MOCK_URL}/_control/reset`)).ok()).toBeTruthy()
}
async function seedFavorite(ctx, thread_id, { res_count, blob_posts }) {
  const r = await ctx.post(`${APP_URL}/_control/seed-favorite`, {
    data: { server: SERVER, board: BOARD, thread_id, res_count, blob_posts },
  })
  expect(r.ok()).toBeTruthy()
}
async function programMockThread(ctx, thread_id, { res_count, dat_posts, gone = false }) {
  const r = await ctx.post(`${MOCK_URL}/_control/thread`, {
    data: { server: SERVER, board: BOARD, thread_id, res_count, dat_posts, gone },
  })
  expect(r.ok()).toBeTruthy()
}
async function subjectHits(ctx, board) {
  const r = await ctx.get(`${MOCK_URL}/_control/subject-hits/${board}`)
  expect(r.ok()).toBeTruthy()
  return r.json()
}
// Counts the posts the stored dat actually holds for a thread (what the viewer would render).
// Returns -1 on a transient non-ok response so `expect.poll` keeps retrying instead of
// throwing on a race.
async function storedPosts(ctx, thread_id) {
  const r = await ctx.get(`${APP_URL}/api/favorites/${SERVER}/${BOARD}/${thread_id}/dat`)
  if (!r.ok()) return -1
  return (await r.json()).res.length
}
// Polls storedPosts until it reaches `want` (background bulk DL is async).
async function waitForStored(ctx, thread_id, want) {
  await expect
    .poll(() => storedPosts(ctx, thread_id), { timeout: 10000 })
    .toBe(want)
}

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

// Board-level bulk prefetch through the REAL backend: three favorites on one board are all
// behind 5ch (blob 100 / subject 130). One /api/favorites/refresh must fetch ALL three grown
// dats and replace their blobs — with subject.txt read EXACTLY ONCE for the board (not per
// thread).
test('refresh bulk-downloads every grown thread on a board with one subject read', async ({
  request,
}) => {
  for (const t of THREADS) {
    await seedFavorite(request, t, { res_count: 100, blob_posts: 100 })
    await programMockThread(request, t, { res_count: 130, dat_posts: 130 })
  }

  // Trigger the board-level refresh (returns immediately; downloads run in the background).
  const r = await request.post(`${APP_URL}/api/favorites/refresh`, { data: {} })
  expect(r.ok()).toBeTruthy()

  // Every thread's stored dat must catch up to 130 (proves all grown dats were fetched).
  for (const t of THREADS) await waitForStored(request, t, 130)

  // subject.txt was read once for the whole board, not once per thread.
  expect(await subjectHits(request, BOARD)).toBe(1)
})

// refresh must NOT fetch dats that did not grow: subject==blob means skip.
test('refresh skips threads whose subject count did not grow', async ({ request }) => {
  await seedFavorite(request, THREADS[0], { res_count: 100, blob_posts: 100 })
  // subject matches the blob -> no growth. Mock would serve 999 IF asked.
  await programMockThread(request, THREADS[0], { res_count: 100, dat_posts: 999 })

  const r = await request.post(`${APP_URL}/api/favorites/refresh`, { data: {} })
  expect(r.ok()).toBeTruthy()

  // Give the background task time to run, then confirm the dat was NOT replaced.
  await expect
    .poll(() => subjectHits(request, BOARD), { timeout: 10000 })
    .toBe(1)
  expect(await storedPosts(request, THREADS[0])).toBe(100)
})

// Note: the open-time board prefetch (spawn_board_prefetch) was removed when the reload
// endpoint was changed to use a HEAD gate instead of subject.txt. The reload endpoint no
// longer reads subject.txt at all, so it has no basis to kick off a board-level prefetch.
// Board-level bulk prefetch is now exclusively triggered by the explicit refresh button
// (POST /api/favorites/refresh), tested in the two tests above.
