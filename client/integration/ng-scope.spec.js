import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL } from '../playwright.integration.config.js'

// Board-scoped NG rules through the REAL Rust router and SQLite. The fast suite
// (tests/ng-word.spec.js) mocks the API, so it cannot catch a broken handler, a wrong
// SQL key, or a route that never got wired — that is what this file pins.
//
// Two boards on the same server hold threads with identical bodies and identical poster
// IDs (ctl_seed writes 本文N / ID:abcN), so a rule that leaked across boards would show up
// as a hidden post on the other board.
const SERVER = 'egg'
const BOARD_A = 'applism'
const BOARD_B = 'other'

const THREAD_A1 = '1771127145'
const THREAD_A2 = '1771127146'
const THREAD_B1 = '1771127147'

async function reset(ctx) {
  const r = await ctx.post(`${APP_URL}/_control/reset`)
  expect(r.ok()).toBeTruthy()
}

async function seedFavorite(ctx, board, thread_id, title) {
  const r = await ctx.post(`${APP_URL}/_control/seed-favorite`, {
    data: { server: SERVER, board, thread_id, title, res_count: 3, blob_posts: 3 },
  })
  expect(r.ok()).toBeTruthy()
}

const ngWordsUrl = (q) => `${APP_URL}/api/ng-words${q ?? ''}`
const ngIdsUrl = (q) => `${APP_URL}/api/ng-ids${q ?? ''}`

async function listJson(ctx, url) {
  const r = await ctx.get(url)
  expect(r.ok()).toBeTruthy()
  return r.json()
}

// Drop created_at (a wall-clock value) so rows can be compared by identity.
const keyOf = ({ created_at, ...rest }) => rest

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

test('NG word add/list/delete round-trips through the real API and SQLite', async ({
  request,
}) => {
  const rule = { server: SERVER, board: BOARD_A, kind: 'text', pattern: '本文2' }

  const added = await request.post(ngWordsUrl(), { data: rule })
  expect(added.ok()).toBeTruthy()

  expect((await listJson(request, ngWordsUrl())).map(keyOf)).toEqual([rule])

  const q = `?server=${SERVER}&board=${BOARD_A}&kind=text&pattern=${encodeURIComponent('本文2')}`
  const removed = await request.delete(ngWordsUrl(q))
  expect(removed.ok()).toBeTruthy()
  expect(await listJson(request, ngWordsUrl())).toEqual([])

  // Deleting a rule that is not there is a 404, not a silent success.
  const again = await request.delete(ngWordsUrl(q))
  expect(again.status()).toBe(404)
})

test('the same NG word on two boards is two independent rules', async ({ request }) => {
  const onA = { server: SERVER, board: BOARD_A, kind: 'text', pattern: '本文2' }
  const onB = { ...onA, board: BOARD_B }
  expect((await request.post(ngWordsUrl(), { data: onA })).ok()).toBeTruthy()
  expect((await request.post(ngWordsUrl(), { data: onB })).ok()).toBeTruthy()

  expect((await listJson(request, ngWordsUrl())).map(keyOf)).toEqual(
    expect.arrayContaining([onA, onB]),
  )

  // Removing board A's rule leaves board B's in place.
  const q = `?server=${SERVER}&board=${BOARD_A}&kind=text&pattern=${encodeURIComponent('本文2')}`
  expect((await request.delete(ngWordsUrl(q))).ok()).toBeTruthy()
  expect((await listJson(request, ngWordsUrl())).map(keyOf)).toEqual([onB])
})

test('re-adding the same scope+kind+pattern keeps a single row', async ({ request }) => {
  const rule = { server: SERVER, board: BOARD_A, kind: 'text', pattern: '本文2' }
  expect((await request.post(ngWordsUrl(), { data: rule })).ok()).toBeTruthy()
  expect((await request.post(ngWordsUrl(), { data: rule })).ok()).toBeTruthy()
  expect(await listJson(request, ngWordsUrl())).toHaveLength(1)

  // A different kind with the same pattern is a separate rule.
  expect((await request.post(ngWordsUrl(), { data: { ...rule, kind: 'regex' } })).ok()).toBeTruthy()
  expect(await listJson(request, ngWordsUrl())).toHaveLength(2)
})

test('an empty pattern and an unknown kind are rejected and store nothing', async ({
  request,
}) => {
  const empty = await request.post(ngWordsUrl(), {
    data: { server: SERVER, board: BOARD_A, kind: 'text', pattern: '' },
  })
  expect(empty.status()).toBe(400)

  const badKind = await request.post(ngWordsUrl(), {
    data: { server: SERVER, board: BOARD_A, kind: 'glob', pattern: '本文2' },
  })
  expect(badKind.status()).toBe(400)

  expect(await listJson(request, ngWordsUrl())).toEqual([])
})

test('an NG ID cannot be created without a board scope', async ({ request }) => {
  // The pre-migration global shape (ng_id alone) is no longer a valid request body.
  const global = await request.post(ngIdsUrl(), { data: { ng_id: 'abc1' } })
  expect(global.ok()).toBeFalsy()
  expect(await listJson(request, ngIdsUrl())).toEqual([])

  const scoped = { server: SERVER, board: BOARD_A, ng_id: 'abc1' }
  expect((await request.post(ngIdsUrl(), { data: scoped })).ok()).toBeTruthy()
  expect((await listJson(request, ngIdsUrl())).map(keyOf)).toEqual([scoped])
})

test('an NG word registered on one board hides its posts in every thread of that board only', async ({
  page,
  request,
}) => {
  await seedFavorite(request, BOARD_A, THREAD_A1, '板Aのスレ1')
  await seedFavorite(request, BOARD_A, THREAD_A2, '板Aのスレ2')
  await seedFavorite(request, BOARD_B, THREAD_B1, '板Bのスレ')
  expect(
    (
      await request.post(ngWordsUrl(), {
        data: { server: SERVER, board: BOARD_A, kind: 'text', pattern: '本文2' },
      })
    ).ok(),
  ).toBeTruthy()

  // Board A, thread 1: post 2 is hidden behind the NG Word disclosure.
  await page.goto(`/${SERVER}/${BOARD_A}/${THREAD_A1}`)
  await expect(page.getByText('本文1', { exact: true })).toBeVisible()
  await expect(page.locator('del.ng').filter({ hasText: '2 NG Word' })).toBeVisible()
  await expect(page.getByText('本文2', { exact: true })).toHaveCount(0)

  // Board A, thread 2: the same rule applies without registering it again.
  await page.goto(`/${SERVER}/${BOARD_A}/${THREAD_A2}`)
  await expect(page.getByText('本文1', { exact: true })).toBeVisible()
  await expect(page.getByText('本文2', { exact: true })).toHaveCount(0)

  // Board B: identical body, no rule — shown.
  await page.goto(`/${SERVER}/${BOARD_B}/${THREAD_B1}`)
  await expect(page.getByText('本文2', { exact: true })).toBeVisible()
  await expect(page.locator('del.ng')).toHaveCount(0)
})

test('an NG ID registered on one board hides its posts on that board only', async ({
  page,
  request,
}) => {
  await seedFavorite(request, BOARD_A, THREAD_A1, '板Aのスレ1')
  await seedFavorite(request, BOARD_B, THREAD_B1, '板Bのスレ')
  // ctl_seed writes ID:abcN on post N, so abc2 identifies post 2 on both boards.
  expect(
    (
      await request.post(ngIdsUrl(), {
        data: { server: SERVER, board: BOARD_A, ng_id: 'abc2' },
      })
    ).ok(),
  ).toBeTruthy()

  await page.goto(`/${SERVER}/${BOARD_A}/${THREAD_A1}`)
  await expect(page.locator('del.ng').filter({ hasText: '2 NG ID' })).toBeVisible()
  await expect(page.getByText('本文2', { exact: true })).toHaveCount(0)

  await page.goto(`/${SERVER}/${BOARD_B}/${THREAD_B1}`)
  await expect(page.getByText('本文2', { exact: true })).toBeVisible()
  await expect(page.locator('del.ng')).toHaveCount(0)
})

test('registering from the modal and removing from the NG menu both re-evaluate the view', async ({
  page,
  request,
}) => {
  await seedFavorite(request, BOARD_A, THREAD_A1, '板Aのスレ1')
  await page.goto(`/${SERVER}/${BOARD_A}/${THREAD_A1}`)
  await expect(page.getByText('本文2', { exact: true })).toBeVisible()

  // Register a word plus the poster ID from the res body's context menu.
  await page.getByText('本文2', { exact: true }).click({ button: 'right' })
  await page.getByRole('button', { name: 'NG Word に追加' }).click()
  await expect(page.locator('[data-testid="ng-word-form"]')).toBeVisible()
  await page.getByRole('button', { name: '追加' }).click()

  // The post is hidden immediately, and both rules reached SQLite with this board's scope.
  await expect(page.getByText('本文2', { exact: true })).toHaveCount(0)
  expect((await listJson(request, ngWordsUrl())).map(keyOf)).toEqual([
    { server: SERVER, board: BOARD_A, kind: 'text', pattern: '本文2' },
  ])
  expect((await listJson(request, ngIdsUrl())).map(keyOf)).toEqual([
    { server: SERVER, board: BOARD_A, ng_id: 'abc2' },
  ])

  // NG ID wins the disclosure; removing it reveals the remaining NG Word reason.
  await page.locator('del.ng').filter({ hasText: '2 NG ID' }).click({ button: 'right' })
  await page.getByRole('button', { name: 'NG IDから削除' }).click()
  await expect(page.locator('del.ng').filter({ hasText: '2 NG Word' })).toBeVisible()
  expect(await listJson(request, ngIdsUrl())).toEqual([])

  // Removing the word too brings the body back.
  await page.locator('del.ng').filter({ hasText: '2 NG Word' }).click({ button: 'right' })
  await page.getByRole('button', { name: 'NG Wordから削除' }).click()
  await expect(page.getByText('本文2', { exact: true })).toBeVisible()
  expect(await listJson(request, ngWordsUrl())).toEqual([])
})
