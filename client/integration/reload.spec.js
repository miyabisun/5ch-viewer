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
  const m = await ctx.post(`${MOCK_URL}/_control/reset`)
  expect(m.ok()).toBeTruthy()
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
async function subjectHits(ctx) {
  const r = await ctx.get(`${MOCK_URL}/_control/subject-hits/${FAV.board}`)
  expect(r.ok()).toBeTruthy()
  return r.json()
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

// Regression (the "stuck at 111" bug), proven through the REAL backend, re-homed
// onto the footer refresh button (ChMate model: entry never touches 5ch):
// the favorite's metadata res_count has drifted to 117 while the stored dat blob holds
// only 111 posts. The mock 5ch reports a dat with 117 posts. Pressing the footer 更新
// button runs the viewer reload (GET): HEAD returns a larger Content-Length than stored,
// so the dat is fetched, the blob is fully replaced, and the view renders all 117 posts.
//
// Note: entry itself does not touch 5ch (verified deterministically in the mock suite
// tests/reload.spec.js). We do not assert the intermediate "111 only" state here because
// the App-level list bulk-refresh (onMount) is a second, legitimate heal trigger that
// races against the real backend; asserting its absence would be flaky. What this test
// pins is the real fetch->DB-replace->getDat heal through the footer button.
test('footer refresh heals a drifted favorite (meta 117 / blob 111 -> 117 posts shown)', async ({
  page,
  request,
}) => {
  // Drifted state: metadata says 117, but the actual stored dat is only 111 posts.
  await seedFavorite(request, { res_count: 117, blob_posts: 111 })
  // 5ch now has 117 posts (HEAD Content-Length will differ from stored blob_posts=111 bytes).
  await programMockThread(request, { res_count: 117, dat_posts: 117 })

  await page.goto('/')
  // Open the thread from the list.
  await page.locator('.info').first().click()

  // Press the footer 更新 button: GET reload runs through the real
  // fetch->DB-replace->getDat flow and the grown dat (117) renders.
  // Scope to the detail pane — the favorites list has its own 更新 button.
  await page.locator('.detail-pane').getByRole('button', { name: '更新' }).click()
  await expect(page.getByText('本文117', { exact: true })).toBeVisible()
  // Sanity: post 112 (beyond the stale 111 ceiling) is also present.
  await expect(page.getByText('本文112', { exact: true })).toBeVisible()
})

// HEAD gate, end to end: when HEAD returns the same Content-Length as stored dat_bytes,
// the dat is NOT re-fetched. subject.txt is never called during a single-thread reload.
test('reload skips the dat fetch when HEAD Content-Length matches stored dat_bytes', async ({
  request,
}) => {
  // Seed 5 posts; ctl_seed computes the Shift-JIS byte length and stores it as dat_bytes.
  await seedFavorite(request, { res_count: 5, blob_posts: 5 })
  // Mock: dat returns the same 5 posts (identical byte size) → HEAD will match.
  await programMockThread(request, { res_count: 5, dat_posts: 5 })

  const result = await reloadThread(request)
  // The gate skipped the full GET.
  expect(result.updated).toBe(false)

  // subject.txt was never fetched (reload uses HEAD, not subject.txt).
  expect(await subjectHits(request)).toBe(0)
})

// HEAD gate: when HEAD Content-Length differs (more posts), the dat IS fetched.
test('reload fetches the dat when HEAD Content-Length grows', async ({ request }) => {
  await seedFavorite(request, { res_count: 5, blob_posts: 5 })
  // Mock serves 10 posts → Content-Length will be larger than what was seeded with 5 posts.
  await programMockThread(request, { res_count: 10, dat_posts: 10 })

  const result = await reloadThread(request)
  expect(result.updated).toBe(true)

  // subject.txt was never fetched (reload uses HEAD, not subject.txt).
  expect(await subjectHits(request)).toBe(0)
})
