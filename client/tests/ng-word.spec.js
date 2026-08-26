import { test, expect } from '@playwright/test'

// Two boards on the same server hold a thread each. Both threads contain the same body
// text and the same poster ID, so a rule registered on one board must not touch the other.
const FAV_A = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '板Aのスレ',
  res_count: 2,
  read_res: 0,
  rating: 0,
  status: 'active',
}
const FAV_B = {
  ...FAV_A,
  board: 'other',
  board_name: '別板',
  thread_id: '1771127146',
  title: '板Bのスレ',
}

const PATH_A = `/${FAV_A.server}/${FAV_A.board}/${FAV_A.thread_id}`
const PATH_B = `/${FAV_B.server}/${FAV_B.board}/${FAV_B.thread_id}`

const ARASHI = '荒らしの本文です'
const NORMAL = 'ふつうの本文'

function datResponse(title) {
  return {
    title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00 ID:arashi',
        body: ARASHI,
        id: 'arashi',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:01:00.00 ID:normal',
        body: NORMAL,
        id: 'normal',
      },
    ],
  }
}

// Mock backend with the real (server, board) scoping: POST appends a scoped row and the
// next GET returns it, so the UI re-evaluates exactly as it would against SQLite.
async function setupRoutes(page, { ngIds = [], ngWords = [] } = {}) {
  const state = { ngIds: [...ngIds], ngWords: [...ngWords] }

  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV_A, FAV_B] }))
  await page.route(/\/api\/favorites\/egg\/applism\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(FAV_A.title) }),
  )
  await page.route(/\/api\/favorites\/egg\/other\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse(FAV_B.title) }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-wacchoi', (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )

  const collection = (name, key) => async (route) => {
    const req = route.request()
    const rows = state[name]
    if (req.method() === 'GET') return route.fulfill({ json: rows })
    if (req.method() === 'POST') {
      const body = req.postDataJSON()
      const dup = rows.some((r) => key.every((k) => r[k] === body[k]))
      if (!dup) rows.push({ ...body, created_at: 0 })
      return route.fulfill({ json: { ok: true } })
    }
    const q = Object.fromEntries(new URL(req.url()).searchParams)
    const i = rows.findIndex((r) => key.every((k) => r[k] === q[k]))
    if (i >= 0) rows.splice(i, 1)
    return route.fulfill({ json: { ok: true } })
  }
  await page.route(/\/api\/ng-ids(\?|$)/, collection('ngIds', ['server', 'board', 'ng_id']))
  await page.route(
    /\/api\/ng-words(\?|$)/,
    collection('ngWords', ['server', 'board', 'kind', 'pattern']),
  )
  return state
}

// Open the NG Word modal from a res body's context menu.
async function openNgWordForm(page, bodyText) {
  await page.getByText(bodyText).click({ button: 'right' })
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
  await page.getByRole('button', { name: 'NG Word に追加' }).click()
  await expect(page.locator('[data-testid="ng-word-form"]')).toBeVisible()
}

const patternField = (page) => page.getByLabel('NG Word')
const alsoIdBox = (page) => page.getByLabel('投稿者IDもNG')

test('the reply menu opens the NG Word modal prefilled with the res body', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(PATH_A)
  await expect(page.getByText(ARASHI)).toBeVisible()

  await openNgWordForm(page, ARASHI)

  // 文字列 is selected by default and the body is the editable initial value.
  await expect(page.getByRole('button', { name: '文字列' })).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: '正規表現' })).toHaveAttribute(
    'aria-pressed',
    'false',
  )
  await expect(patternField(page)).toHaveValue(ARASHI)
  await expect(alsoIdBox(page)).toBeChecked()
  await expect(alsoIdBox(page)).toBeEnabled()
})

test('a long-press on the res body also reaches the NG Word modal', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(PATH_A)
  const body = page.getByText(ARASHI)
  await expect(body).toBeVisible()

  await body.dispatchEvent('pointerdown', { pointerType: 'touch', pointerId: 1 })
  await page.waitForTimeout(550)
  await body.dispatchEvent('pointerup', { pointerType: 'touch', pointerId: 1 })

  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
  await page.getByRole('button', { name: 'NG Word に追加' }).click()
  await expect(page.locator('[data-testid="ng-word-form"]')).toBeVisible()
})

test('キャンセル closes the modal without saving anything', async ({ page }) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  await page.getByRole('button', { name: 'キャンセル' }).click()
  await expect(page.locator('[data-testid="ng-word-form"]')).toHaveCount(0)
  expect(state.ngWords).toEqual([])
  expect(state.ngIds).toEqual([])
})

test('a literal rule hides the post right away and also registers the poster ID', async ({
  page,
}) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  // Narrow the prefilled body down to the substring that should match.
  await patternField(page).fill('荒らし')
  await page.getByRole('button', { name: '追加' }).click()

  // The modal closes and the post is hidden without a manual refresh. Both rules now
  // match it, and NG ID is the reason shown (see the precedence test below).
  await expect(page.locator('[data-testid="ng-word-form"]')).toHaveCount(0)
  await expect(page.getByText(ARASHI)).toHaveCount(0)
  await expect(page.locator('del.ng').filter({ hasText: '1 NG ID' })).toBeVisible()
  // The unrelated post stays visible.
  await expect(page.getByText(NORMAL)).toBeVisible()

  // Both rules were saved for this board only.
  expect(state.ngWords).toEqual([
    { server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 },
  ])
  expect(state.ngIds).toEqual([{ server: 'egg', board: 'applism', ng_id: 'arashi', created_at: 0 }])
})

test('unchecking 投稿者IDもNG saves the word without the ID', async ({ page }) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  await patternField(page).fill('荒らし')
  await alsoIdBox(page).uncheck()
  await page.getByRole('button', { name: '追加' }).click()

  // The word alone hides the post, so NG Word is the reason shown.
  await expect(page.getByText(ARASHI)).toHaveCount(0)
  await expect(page.locator('del.ng').filter({ hasText: '1 NG Word' })).toBeVisible()
  expect(state.ngWords).toHaveLength(1)
  expect(state.ngIds).toEqual([])
})

test('a regex rule is stored with kind=regex and hides matching posts', async ({ page }) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  await page.getByRole('button', { name: '正規表現' }).click()
  await expect(page.getByRole('button', { name: '正規表現' })).toHaveAttribute(
    'aria-pressed',
    'true',
  )
  await patternField(page).fill('^荒ら.+です$')
  // Word only: with the ID also registered, the ID rule would hide the post on its own
  // and this would not prove the regex matched anything.
  await alsoIdBox(page).uncheck()
  await page.getByRole('button', { name: '追加' }).click()

  await expect(page.getByText(ARASHI)).toHaveCount(0)
  await expect(page.locator('del.ng').filter({ hasText: '1 NG Word' })).toBeVisible()
  await expect(page.getByText(NORMAL)).toBeVisible()
  expect(state.ngWords).toEqual([
    { server: 'egg', board: 'applism', kind: 'regex', pattern: '^荒ら.+です$', created_at: 0 },
  ])
  expect(state.ngIds).toEqual([])
})

test('an invalid regex is rejected with an error and nothing is saved', async ({ page }) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  await page.getByRole('button', { name: '正規表現' }).click()
  await patternField(page).fill('荒らし(')
  await page.getByRole('button', { name: '追加' }).click()

  await expect(page.getByRole('alert')).toHaveText('正規表現として解釈できません')
  await expect(page.locator('[data-testid="ng-word-form"]')).toBeVisible()
  expect(state.ngWords).toEqual([])
  expect(state.ngIds).toEqual([])
  // The same text is a perfectly good literal, so switching kind clears the error.
  await page.getByRole('button', { name: '文字列' }).click()
  await expect(page.getByRole('alert')).toHaveCount(0)
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.locator('[data-testid="ng-word-form"]')).toHaveCount(0)
  expect(state.ngWords).toHaveLength(1)
})

test('an empty pattern is rejected with an error and nothing is saved', async ({ page }) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  await patternField(page).fill('')
  await page.getByRole('button', { name: '追加' }).click()

  await expect(page.getByRole('alert')).toHaveText('パターンを入力してください')
  expect(state.ngWords).toEqual([])
})

test('the ID checkbox is disabled and off for a post without a poster ID', async ({ page }) => {
  await setupRoutes(page)
  await page.route(/\/api\/favorites\/egg\/applism\/.+\/dat$/, (route) =>
    route.fulfill({
      json: {
        title: FAV_A.title,
        res_count: 1,
        read_res: 0,
        status: 'active',
        res: [
          { num: 1, name: '名無し', mail: '', date: '2025/01/01(水) 00:00:00.00', body: ARASHI },
        ],
      },
    }),
  )
  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)

  await expect(alsoIdBox(page)).toBeDisabled()
  await expect(alsoIdBox(page)).not.toBeChecked()
})

test('adding the same rule twice is idempotent', async ({ page }) => {
  const state = await setupRoutes(page)
  await page.goto(PATH_A)

  // Registered from the other post's menu (a hidden post's card menu is the removal
  // menu instead), so the same rule can be submitted twice.
  await openNgWordForm(page, NORMAL)
  await patternField(page).fill('荒らし')
  await alsoIdBox(page).uncheck()
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.getByText(ARASHI)).toHaveCount(0)

  await openNgWordForm(page, NORMAL)
  await patternField(page).fill('荒らし')
  await alsoIdBox(page).uncheck()
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.locator('[data-testid="ng-word-form"]')).toHaveCount(0)

  expect(state.ngWords).toHaveLength(1)
})

test('a rule registered on one board does not hide the same body on another board', async ({
  page,
}) => {
  await setupRoutes(page, {
    ngWords: [{ server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 }],
  })

  // Board A: the rule's own board — hidden.
  await page.goto(PATH_A)
  await expect(page.locator('del.ng').filter({ hasText: '1 NG Word' })).toBeVisible()
  await expect(page.getByText(ARASHI)).toHaveCount(0)

  // Board B: same server, same body text, no rule — shown.
  await page.goto(PATH_B)
  await expect(page.getByText(ARASHI)).toBeVisible()
  await expect(page.locator('del.ng')).toHaveCount(0)
})

test('an NG ID registered on one board does not hide the same ID on another board', async ({
  page,
}) => {
  await setupRoutes(page, {
    ngIds: [{ server: 'egg', board: 'applism', ng_id: 'arashi', created_at: 0 }],
  })

  await page.goto(PATH_A)
  await expect(page.locator('del.ng').filter({ hasText: '1 NG ID' })).toBeVisible()

  await page.goto(PATH_B)
  await expect(page.getByText(ARASHI)).toBeVisible()
  await expect(page.locator('del.ng')).toHaveCount(0)
})

test('a rule applies to every thread of its board, including one opened later', async ({
  page,
}) => {
  // Two threads on board A; the rule is added from the first one.
  const SECOND = { ...FAV_A, thread_id: '1771127147', title: '板Aの別スレ' }
  await setupRoutes(page)
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV_A, SECOND, FAV_B] }))

  await page.goto(PATH_A)
  await openNgWordForm(page, ARASHI)
  await patternField(page).fill('荒らし')
  // Word only, so it is the word — not the ID — that is shown to cross threads.
  await alsoIdBox(page).uncheck()
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.getByText(ARASHI)).toHaveCount(0)

  // The other thread of the same board is hidden too, without re-registering.
  await page.goto(`/${FAV_A.server}/${FAV_A.board}/${SECOND.thread_id}`)
  await expect(page.locator('del.ng').filter({ hasText: '1 NG Word' })).toBeVisible()
  await expect(page.getByText(ARASHI)).toHaveCount(0)
})

test('the hidden post offers NG Word removal and the body comes back', async ({ page }) => {
  const state = await setupRoutes(page, {
    ngWords: [{ server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 }],
  })
  await page.goto(PATH_A)

  const header = page.locator('del.ng').filter({ hasText: '1 NG Word' })
  await expect(header).toBeVisible()

  await header.click({ button: 'right' })
  await expect(page.locator('[data-testid="ng-menu"]')).toBeVisible()
  await page.getByRole('button', { name: 'NG Wordから削除' }).click()

  // The rule is gone and the post is re-evaluated back into view.
  await expect(page.getByText(ARASHI)).toBeVisible()
  await expect(page.locator('del.ng')).toHaveCount(0)
  expect(state.ngWords).toEqual([])
})

test('NG ID takes precedence, and removing it reveals the remaining NG Word reason', async ({
  page,
}) => {
  await setupRoutes(page, {
    ngIds: [{ server: 'egg', board: 'applism', ng_id: 'arashi', created_at: 0 }],
    ngWords: [{ server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 }],
  })
  await page.goto(PATH_A)

  // Both rules match; the ID reason is the one shown.
  const idHeader = page.locator('del.ng').filter({ hasText: '1 NG ID' })
  await expect(idHeader).toBeVisible()

  await idHeader.click({ button: 'right' })
  await page.getByRole('button', { name: 'NG IDから削除' }).click()

  // The post stays hidden, now under the NG Word reason.
  await expect(page.locator('del.ng').filter({ hasText: '1 NG Word' })).toBeVisible()
  await expect(page.getByText(ARASHI)).toHaveCount(0)
})

test('a rule matches the display text, not the body markup', async ({ page }) => {
  await setupRoutes(page)
  await page.route(/\/api\/favorites\/egg\/applism\/.+\/dat$/, (route) =>
    route.fulfill({
      json: {
        title: FAV_A.title,
        res_count: 1,
        read_res: 0,
        status: 'active',
        res: [
          {
            num: 1,
            name: '名無し',
            mail: '',
            date: '2025/01/01(水) 00:00:00.00 ID:arashi',
            // Entities and a <br> the reader never sees as markup.
            body: '1行目<br>a &amp; b',
            id: 'arashi',
          },
        ],
      },
    }),
  )
  await page.goto(PATH_A)
  await openNgWordForm(page, '1行目')

  // The prefilled pattern is what is on screen: decoded entity, newline for <br>.
  await expect(patternField(page)).toHaveValue('1行目\na & b')

  // A literal containing the decoded entity matches; the raw markup does not.
  await patternField(page).fill('a & b')
  await alsoIdBox(page).uncheck()
  await page.getByRole('button', { name: '追加' }).click()
  await expect(page.locator('del.ng').filter({ hasText: '1 NG Word' })).toBeVisible()
})
