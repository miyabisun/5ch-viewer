import { test, expect } from '@playwright/test'

const FAVS = [
  {
    server: 'egg',
    board: 'applism',
    board_name: 'アプリ',
    thread_id: '1000000001',
    title: 'スレA',
    res_count: 10,
    read_res: 5,
    rating: 0,
    status: 'active',
  },
  {
    server: 'egg',
    board: 'applism',
    board_name: 'アプリ',
    thread_id: '1000000002',
    title: 'スレB',
    res_count: 20,
    read_res: 20,
    rating: 0,
    status: 'active',
  },
]

// Two boards, multiple threads each — for accordion grouping test.
const ARCHIVES = [
  {
    server: 'egg',
    board: 'applism',
    board_name: 'アプリ',
    thread_id: '2000000001',
    title: 'アーカイブA',
    res_count: 10,
    read_res: 8,
    rating: 0,
    status: 'active',
  },
  {
    server: 'egg',
    board: 'applism',
    board_name: 'アプリ',
    thread_id: '2000000002',
    title: 'アーカイブB',
    res_count: 5,
    read_res: 5,
    rating: 0,
    status: 'active',
  },
  {
    server: 'egg',
    board: 'gekikara',
    board_name: 'テスト板',
    thread_id: '3000000001',
    title: 'アーカイブC',
    res_count: 3,
    read_res: 0,
    rating: 0,
    status: 'active',
  },
]

function mockBase(page) {
  page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
}

// ---- Tab navigation ----

test('archive tab is visible and switches to /archive', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: [] }))
  await page.goto('/')

  await expect(page.getByTestId('tab-archive')).toBeVisible()
  await page.getByTestId('tab-archive').click()
  await expect(page.getByTestId('tab-archive')).toHaveClass(/active/)
  await expect(page).toHaveURL('/archive')
})

test('archive tab is not active on favorites page', async ({ page }) => {
  await mockBase(page)
  await page.goto('/')

  await expect(page.getByTestId('tab-favorites')).toHaveClass(/active/)
  await expect(page.getByTestId('tab-archive')).not.toHaveClass(/active/)
})

// ---- FavoritesList: archive button ----

test('favorites menu has アーカイブ button above 削除', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: [] }))
  await page.goto('/')

  await page.locator('.thread').first().click({ button: 'right' })
  await expect(page.locator('.menu')).toBeVisible()

  const menu = page.locator('.menu')
  const archiveBtn = menu.getByRole('button', { name: 'アーカイブ' })
  const deleteBtn = menu.getByRole('button', { name: '削除' })
  await expect(archiveBtn).toBeVisible()
  await expect(deleteBtn).toBeVisible()

  // アーカイブ must appear before 削除 in DOM order.
  const archiveIndex = await archiveBtn.evaluate((el) => {
    const buttons = [...el.closest('.menu').querySelectorAll('button')]
    return buttons.indexOf(el)
  })
  const deleteIndex = await deleteBtn.evaluate((el) => {
    const buttons = [...el.closest('.menu').querySelectorAll('button')]
    return buttons.indexOf(el)
  })
  expect(archiveIndex).toBeLessThan(deleteIndex)
})

test('pressing アーカイブ sends PATCH .../archived with {archived:true}', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: [] }))
  const sent = []
  await page.route(/\/api\/favorites\/.+\/archived$/, (route) => {
    sent.push(route.request().postDataJSON())
    route.fulfill({ json: { ok: true } })
  })
  await page.goto('/')

  await page.locator('.thread').first().click({ button: 'right' })
  await page.locator('.menu').getByRole('button', { name: 'アーカイブ' }).click()

  await expect.poll(() => sent.length).toBe(1)
  expect(sent[0]).toEqual({ archived: true })

  // Menu closes after the action.
  await expect(page.locator('.menu')).toHaveCount(0)
})

// ---- ArchiveList: accordion ----

test('archive page shows board accordion collapsed by default', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: ARCHIVES }))
  await page.goto('/archive')

  // Board headers visible.
  await expect(page.locator('.board-header')).toHaveCount(2)

  // Thread rows are not visible (accordion collapsed).
  await expect(page.locator('.thread')).toHaveCount(0)
})

test('clicking board header expands accordion and shows threads', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: ARCHIVES }))
  await page.goto('/archive')

  // Expand the first group (アプリ — 2 threads).
  await page.locator('.board-header').first().click()
  await expect(page.locator('.thread')).toHaveCount(2)

  // Collapse again.
  await page.locator('.board-header').first().click()
  await expect(page.locator('.thread')).toHaveCount(0)
})

test('board header shows board name and thread count', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: ARCHIVES }))
  await page.goto('/archive')

  const headers = page.locator('.board-header')
  // Two boards: アプリ(2) and テスト板(1). Natural sort puts アプリ before テスト板.
  await expect(headers.nth(0).locator('.board-name')).toHaveText('アプリ')
  await expect(headers.nth(0).locator('.board-count')).toHaveText('(2)')
  await expect(headers.nth(1).locator('.board-name')).toHaveText('テスト板')
  await expect(headers.nth(1).locator('.board-count')).toHaveText('(1)')
})

test('unread badge shown for archive threads with unread posts', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: ARCHIVES }))
  await page.goto('/archive')

  // Expand アプリ group.
  await page.locator('.board-header').first().click()

  // アーカイブA has res_count=10, read_res=8 → 2 unread.
  const badges = page.locator('.unread')
  await expect(badges).toHaveCount(1)
  await expect(badges.first()).toHaveText('2')
})

test('clicking archive thread row calls onopen (navigates to thread)', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: ARCHIVES }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({
      json: {
        title: 'アーカイブA',
        res_count: 10,
        read_res: 8,
        status: 'active',
        res: [
          {
            num: 1,
            name: '名無し',
            mail: '',
            date: '2025 ID:x',
            body: 'アーカイブ本文',
          },
        ],
      },
    }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 10, read_res: 8, status: 'active' } }),
  )
  await page.goto('/archive')

  await page.locator('.board-header').first().click()
  await page.locator('.thread .info').first().click()
  await expect(page.getByText('アーカイブ本文')).toBeVisible()
})

test('archive list アーカイブ解除 sends PATCH .../archived with {archived:false}', async ({
  page,
}) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: ARCHIVES }))
  const sent = []
  await page.route(/\/api\/favorites\/.+\/archived$/, (route) => {
    sent.push(route.request().postDataJSON())
    route.fulfill({ json: { ok: true } })
  })
  await page.goto('/archive')

  // Expand first group and right-click the first thread.
  await page.locator('.board-header').first().click()
  await page.locator('.thread').first().click({ button: 'right' })
  await expect(page.locator('.menu')).toBeVisible()

  await page.getByRole('button', { name: 'アーカイブ解除' }).click()
  await expect.poll(() => sent.length).toBe(1)
  expect(sent[0]).toEqual({ archived: false })
})

test('empty archive page shows placeholder message', async ({ page }) => {
  await mockBase(page)
  await page.route('**/api/archives', (route) => route.fulfill({ json: [] }))
  await page.goto('/archive')

  await expect(page.locator('.empty')).toHaveText('アーカイブはありません')
})
