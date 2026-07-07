import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL, MOCK_URL } from '../playwright.integration.config.js'

const SERVER = 'egg'
const BOARD = 'applism'
// Source (dead) thread and its successor (Part number +1).
const SRC = '1771127100'
const NEXT = '1771127101'
const SRC_TITLE = 'ブルアカ Part5862'
const NEXT_TITLE = 'ブルアカ Part5863'

async function reset(ctx) {
  expect((await ctx.post(`${APP_URL}/_control/reset`)).ok()).toBeTruthy()
  expect((await ctx.post(`${MOCK_URL}/_control/reset`)).ok()).toBeTruthy()
}
async function seedFavorite(ctx, thread_id, { title, res_count = 1002 }) {
  const r = await ctx.post(`${APP_URL}/_control/seed-favorite`, {
    data: { server: SERVER, board: BOARD, thread_id, title, res_count, blob_posts: 1 },
  })
  expect(r.ok()).toBeTruthy()
}
async function programMockThread(ctx, thread_id, { title, res_count }) {
  const r = await ctx.post(`${MOCK_URL}/_control/thread`, {
    data: { server: SERVER, board: BOARD, thread_id, title, res_count, dat_posts: 1 },
  })
  expect(r.ok()).toBeTruthy()
}
async function favorites(ctx) {
  const r = await ctx.get(`${APP_URL}/api/favorites`)
  expect(r.ok()).toBeTruthy()
  return r.json()
}

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

// The manual rescue: source thread went dead; its successor is present in subject.txt.
// POST find-next must fetch subject once, register the next thread, and return found:true.
test('find-next registers the successor thread and returns found:true', async ({ request }) => {
  await seedFavorite(request, SRC, { title: SRC_TITLE })
  // Only the successor is listed in subject (the source already dropped off).
  await programMockThread(request, NEXT, { title: NEXT_TITLE, res_count: 5 })

  const r = await request.post(`${APP_URL}/api/favorites/${SERVER}/${BOARD}/${SRC}/find-next`)
  expect(r.ok()).toBeTruthy()
  const body = await r.json()
  expect(body.found).toBe(true)
  expect(body.thread_id).toBe(NEXT)
  expect(body.title).toBe(NEXT_TITLE)

  // The successor is now a real favorite row.
  const list = await favorites(request)
  const found = list.find((f) => f.thread_id === NEXT)
  expect(found).toBeTruthy()
  expect(found.title).toBe(NEXT_TITLE)
})

// No successor posted yet -> found:false and nothing new is registered.
test('find-next returns found:false when no successor is in subject', async ({ request }) => {
  await seedFavorite(request, SRC, { title: SRC_TITLE })
  // Subject is empty (no next thread).

  const r = await request.post(`${APP_URL}/api/favorites/${SERVER}/${BOARD}/${SRC}/find-next`)
  expect(r.ok()).toBeTruthy()
  expect((await r.json()).found).toBe(false)

  const list = await favorites(request)
  expect(list.find((f) => f.thread_id === NEXT)).toBeFalsy()
})
