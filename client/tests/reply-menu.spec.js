import { test, expect, devices } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '返信メニューテストスレ',
  res_count: 2,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

// res[0].body carries a <br> so the "本文をコピー" plain-text conversion (<br> -> \n)
// can be verified. Bodies are server-sanitized to <a>/<br> only.
function datResponse() {
  return {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      { num: 1, name: '名無し', mail: '', date: '2025/01/01(水) 00:00:00.00', body: '一行目<br>二行目', id: null },
      { num: 2, name: '名無し', mail: '', date: '2025/01/01(水) 00:01:00.00', body: '本文2', id: null },
    ],
  }
}

async function setupRoutes(page) {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route(/\/api\/ng-ids\/.*/, (route) => route.fulfill({ json: { ok: true } }))
  await page.route('**/api/ng-wacchoi', (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
}

// --- Desktop context (default fine pointer): selection is preserved ---

test('desktop: .body keeps text selection (user-select is not none)', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('二行目')).toBeVisible()

  const userSelect = await page
    .locator('.body')
    .first()
    .evaluate((el) => getComputedStyle(el).userSelect)
  expect(userSelect).not.toBe('none')
})

test('desktop: right-click on .body opens the reply menu', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('二行目')).toBeVisible()

  await page.locator('.body').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
})

test('reply menu keeps the 返信する action and opens the post modal', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('二行目')).toBeVisible()

  await page.locator('.body').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()

  await page.getByRole('button', { name: '返信する' }).click()
  // startReply prefills the textarea with ">>1".
  await expect(page.locator('.post-textarea')).toHaveValue('>>1\n')
})

test('本文をコピー writes the body (with newline) to the clipboard', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'])
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('二行目')).toBeVisible()

  await page.locator('.body').first().click({ button: 'right' })
  await page.getByRole('button', { name: '本文をコピー' }).click()

  const text = await page.evaluate(() => navigator.clipboard.readText())
  // <br> becomes a newline; text is plain (tags stripped, entities decoded).
  expect(text).toBe('一行目\n二行目')
})

// --- Mobile context (touch, pointer: coarse): selection is suppressed ---

// Strip defaultBrowserType from the device descriptor: setting it inside a
// describe forces a new worker, which Playwright rejects. The remaining fields
// (hasTouch + isMobile) yield the pointer: coarse / hover: none emulation the
// touch-only CSS depends on.
const { defaultBrowserType, ...pixel5 } = devices['Pixel 5']

test.describe('mobile (touch)', () => {
  test.use(pixel5)

  test('.body computed user-select is none', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('二行目')).toBeVisible()

    const userSelect = await page
      .locator('.body')
      .first()
      .evaluate((el) => getComputedStyle(el).userSelect)
    expect(userSelect).toBe('none')
  })

  // Chromium drops -webkit-touch-callout entirely (unknown non-WebKit property):
  // it is absent from both getComputedStyle and the parsed CSSOM. The only
  // Chromium-observable proof is the raw built stylesheet source, so we fetch it
  // and confirm the .body rule ships -webkit-touch-callout: none.
  test('.body ships -webkit-touch-callout: none in the built CSS', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('二行目')).toBeVisible()

    const css = await page.evaluate(async () => {
      const hrefs = [...document.styleSheets].map((s) => s.href).filter(Boolean)
      const texts = await Promise.all(hrefs.map((h) => fetch(h).then((r) => r.text())))
      return texts.join('\n')
    })
    // The scoped .body rule (any Svelte hash) must declare the callout suppression.
    expect(css).toMatch(/\.body[^{}]*\{[^}]*-webkit-touch-callout:\s*none/)
  })
})
