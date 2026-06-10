import { test, expect } from '@playwright/test'

// One favorite per rating 0..5, so we can assert the color-bar class per item.
const FAVS = [0, 1, 2, 3, 4, 5].map((r) => ({
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: `100000000${r}`,
  title: `スレ${r}`,
  res_count: 10,
  read_res: 10,
  rating: r,
  status: 'active',
}))

function ratingRoute(page, store) {
  return page.route(/\/api\/favorites\/.+\/rating$/, (route) => {
    store.push(route.request().postDataJSON())
    route.fulfill({ json: { ok: true } })
  })
}

test('each item carries its rating color-bar class (0..5)', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.goto('/')

  for (const r of [0, 1, 2, 3, 4, 5]) {
    const item = page.locator(`.thread.rate-${r}`)
    await expect(item).toHaveCount(1)
    await expect(item).toHaveAttribute('data-rating', String(r))
    // The bar is the left border colored from --rate-{r}.
    const color = await item.evaluate((el) => getComputedStyle(el).borderLeftColor)
    expect(color).not.toBe('rgba(0, 0, 0, 0)')
    expect(color).not.toBe('')
  }
})

test('right-click opens the action menu and rating change is sent', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  const sent = []
  await ratingRoute(page, sent)
  await page.goto('/')

  // Menu is hidden until invoked.
  await expect(page.locator('.menu')).toHaveCount(0)

  await page.locator('.thread.rate-0').click({ button: 'right' })
  await expect(page.locator('.menu')).toBeVisible()

  // The URL row is shown so the user knows what "URL をコピー" copies.
  await expect(page.locator('.menu-url')).toHaveText(
    'https://egg.5ch.io/test/read.cgi/applism/1000000000/',
  )

  // Change the rating to ★4 by clicking the 4th star.
  await page.locator('.star[data-rating="4"]').click()
  await expect.poll(() => sent.length).toBe(1)
  expect(sent[0]).toEqual({ rating: 4 })

  // Menu closes after the action.
  await expect(page.locator('.menu')).toHaveCount(0)
})

test('stars reflect the current rating with color, and modal closes via × / scrim', async ({
  page,
}) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.goto('/')

  // ★3 item: stars 1..3 lit, 4..5 off — selection shown by color (not underline).
  await page.locator('.thread.rate-3').click({ button: 'right' })
  await expect(page.locator('.star.on')).toHaveCount(3)
  await expect(page.locator('.star.off')).toHaveCount(2)
  await expect(page.locator('.star[data-rating="3"]')).toHaveText('★')
  await expect(page.locator('.star[data-rating="4"]')).toHaveText('☆')

  // Close via the top-right ×.
  await page.getByRole('button', { name: '閉じる' }).click()
  await expect(page.locator('.menu')).toHaveCount(0)

  // Reopen and close via a scrim (outside) click.
  await page.locator('.thread.rate-3').click({ button: 'right' })
  await expect(page.locator('.menu')).toBeVisible()
  await page.locator('.modal-bg').click({ position: { x: 5, y: 5 } })
  await expect(page.locator('.menu')).toHaveCount(0)

  // The dropped bottom "閉じる" button must not exist (only the × remains).
  await page.locator('.thread.rate-3').click({ button: 'right' })
  await expect(page.locator('.action.close')).toHaveCount(0)
})

test('unread badge: shown (rounded, colored) when unread > 0, hidden at 0', async ({
  page,
}) => {
  const favs = [
    { ...FAVS[3], thread_id: '2000000001', title: 'unread', res_count: 10, read_res: 4 },
    { ...FAVS[3], thread_id: '2000000002', title: 'read', res_count: 10, read_res: 10 },
  ]
  await page.route('**/api/favorites', (route) => route.fulfill({ json: favs }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.goto('/')

  // Exactly one badge (the unread item); the fully-read item shows none.
  await expect(page.locator('.unread')).toHaveCount(1)
  const badge = page.locator('.unread')
  await expect(badge).toHaveText('6')

  const style = await badge.evaluate((el) => {
    const s = getComputedStyle(el)
    return { bg: s.backgroundColor, radius: s.borderTopLeftRadius }
  })
  // Dark-red background (not transparent) and a rounded pill.
  expect(style.bg).not.toBe('rgba(0, 0, 0, 0)')
  expect(parseFloat(style.radius)).toBeGreaterThan(0)
})

test('copy actions write title / url / share text to the clipboard', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.goto('/')

  const url = 'https://egg.5ch.io/test/read.cgi/applism/1000000003/'
  const readClipboard = () => page.evaluate(() => navigator.clipboard.readText())

  // Title only.
  await page.locator('.thread.rate-3').click({ button: 'right' })
  await page.getByRole('button', { name: 'タイトルをコピー' }).click()
  expect(await readClipboard()).toBe('スレ3')

  // URL only.
  await page.locator('.thread.rate-3').click({ button: 'right' })
  await page.getByRole('button', { name: 'URL をコピー', exact: true }).click()
  expect(await readClipboard()).toBe(url)

  // Title + URL (share for a new thread).
  await page.locator('.thread.rate-3').click({ button: 'right' })
  await page.getByRole('button', { name: 'タイトル+URL をコピー' }).click()
  expect(await readClipboard()).toBe(`スレ3\n${url}`)
})

test('plain click still opens the thread', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: FAVS }))
  await page.route('**/api/favorites/refresh', (route) => route.fulfill({ json: { ok: true, boards: 0 } }))
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({
      json: {
        title: 'スレ3',
        res_count: 1,
        read_res: 0,
        status: 'active',
        res: [{ num: 1, name: '名無し', mail: '', date: '2025 ID:x', body: '本文1' }],
      },
    }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 1, read_res: 0, status: 'active' } }),
  )
  await page.goto('/')

  await page.locator('.thread.rate-3 .info').click()
  await expect(page.getByText('本文1')).toBeVisible()
})
