import { test, expect } from '@playwright/test'

const SRC = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127100',
  title: 'ブルアカ Part5862',
  res_count: 1002,
  read_res: 0,
  rating: 3,
  status: 'dead',
}
const NEXT = {
  ...SRC,
  thread_id: '1771127101',
  title: 'ブルアカ Part5863',
  res_count: 5,
  status: 'active',
}

test('menu has "次スレを検索"; on found it fires find-next once and the new thread appears', async ({
  page,
}) => {
  // The favorites list grows to include the successor after find-next reports found.
  let favState = [SRC]
  await page.route('**/api/favorites', (route) => route.fulfill({ json: favState }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  let findNextHits = 0
  await page.route(/\/find-next$/, (route) => {
    findNextHits += 1
    favState = [SRC, NEXT]
    route.fulfill({
      json: {
        found: true,
        server: NEXT.server,
        board: NEXT.board,
        thread_id: NEXT.thread_id,
        title: NEXT.title,
      },
    })
  })

  await page.goto('/')

  await page.locator('.thread.rate-3').click({ button: 'right' })
  await expect(page.locator('.menu')).toBeVisible()

  const findBtn = page.getByRole('button', { name: '次スレを検索' })
  await expect(findBtn).toBeVisible()

  await findBtn.click()

  // Exactly one find-next request fires.
  await expect.poll(() => findNextHits).toBe(1)

  // After the list reloads, the successor thread is shown.
  await expect(page.getByText('ブルアカ Part5863')).toBeVisible()
})

test('on found:false the menu shows inline feedback and stays open', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [SRC] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/find-next$/, (route) => route.fulfill({ json: { found: false } }))

  await page.goto('/')

  await page.locator('.thread.rate-3').click({ button: 'right' })
  await page.getByRole('button', { name: '次スレを検索' }).click()

  // Inline feedback appears and the menu remains open.
  await expect(page.getByTestId('find-next-status')).toHaveText('次スレは見つかりませんでした')
  await expect(page.locator('.menu')).toBeVisible()
})
