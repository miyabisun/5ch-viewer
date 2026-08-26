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
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00',
        body: '一行目<br>二行目',
        id: 'TESTID1',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:01:00.00',
        body: '短文<br>https://example.com/test.jpg',
        id: null,
      },
    ],
  }
}

async function setupRoutes(page) {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.route(/\/api\/favorites\/.+\/dat$/, (route) => route.fulfill({ json: datResponse() }))
  await page.route(/\/api\/favorites\/.+\/reload$/, (route) =>
    route.fulfill({ json: { res_count: 2, read_res: 0, status: 'active' } }),
  )
  await page.route('**/api/ng-ids', (route) =>
    route.fulfill({
      json: route.request().method() === 'GET' ? [] : { ok: true },
    }),
  )
  await page.route(/\/api\/ng-words(\?|$)/, (route) =>
    route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } }),
  )
  await page.route('**/api/ng-wacchoi', (route) =>
    route.fulfill({
      json: route.request().method() === 'GET' ? [] : { ok: true },
    }),
  )
  await page.route('**/api/images/**', (route) => route.fulfill({ status: 200, body: '' }))
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

  await page.locator('.res[data-res="1"] .body').click({ button: 'right' })
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
})

test('desktop: right-click on the res header opens the reply menu', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('二行目')).toBeVisible()

  await page.locator('.res .num').first().click({ button: 'right' })
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
})

test('desktop: right-click on image-strip whitespace opens the reply menu', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.locator('.thumb-strip')).toBeVisible()

  await page.locator('.thumb-strip').dispatchEvent('contextmenu')
  await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
})

test('desktop: an image context menu does not bubble into the res menu', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.locator('.thumb-btn')).toBeVisible()

  await page.locator('.thumb-btn').click({ button: 'right' })
  await expect(page.locator('[data-testid="image-menu"]')).toBeVisible()
  await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)
})

test('reply menu keeps the 返信する action and opens the post modal', async ({ page }) => {
  await setupRoutes(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('二行目')).toBeVisible()

  await page.locator('.res[data-res="1"] .body').click({ button: 'right' })
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

  await page.locator('.res[data-res="1"] .body').click({ button: 'right' })
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

  test('long-press on the res header opens the reply menu', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('二行目')).toBeVisible()

    const num = page.locator('.res .num').first()
    await num.dispatchEvent('pointerdown', {
      pointerType: 'touch',
      pointerId: 1,
    })
    await page.waitForTimeout(550)
    await num.dispatchEvent('pointerup', {
      pointerType: 'touch',
      pointerId: 1,
    })

    await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
  })

  test('long-press on image-strip whitespace opens the reply menu', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.locator('.thumb-strip')).toBeVisible()

    const strip = page.locator('.thumb-strip')
    await strip.dispatchEvent('pointerdown', {
      pointerType: 'touch',
      pointerId: 1,
    })
    await page.waitForTimeout(550)
    await strip.dispatchEvent('pointerup', {
      pointerType: 'touch',
      pointerId: 1,
    })

    await expect(page.locator('[data-testid="reply-menu"]')).toBeVisible()
  })

  test('long-press on an image opens only the image menu', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.locator('.thumb-btn')).toBeVisible()

    const thumb = page.locator('.thumb-btn')
    await thumb.dispatchEvent('pointerdown', {
      pointerType: 'touch',
      pointerId: 1,
    })
    await page.waitForTimeout(550)
    await thumb.dispatchEvent('pointerup', {
      pointerType: 'touch',
      pointerId: 1,
    })

    await expect(page.locator('[data-testid="image-menu"]')).toBeVisible()
    await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)
  })

  test('long-press on an ID opens only the ID menu', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.locator('.id-badge').first()).toBeVisible()

    const badge = page.locator('.id-badge').first()
    await badge.dispatchEvent('pointerdown', {
      pointerType: 'touch',
      pointerId: 1,
    })
    await page.waitForTimeout(550)
    await badge.dispatchEvent('pointerup', {
      pointerType: 'touch',
      pointerId: 1,
    })

    await expect(page.locator('[data-testid="id-menu"]')).toBeVisible()
    await expect(page.locator('[data-testid="reply-menu"]')).toHaveCount(0)
  })

  // Chromium drops -webkit-touch-callout entirely (unknown non-WebKit property):
  // it is absent from both getComputedStyle and the parsed CSSOM. The only
  // Chromium-observable proof is the raw built stylesheet source, so we fetch it
  // and confirm the .res rule ships -webkit-touch-callout: none.
  test('.res ships -webkit-touch-callout: none in the built CSS', async ({ page }) => {
    await setupRoutes(page)
    await page.goto(THREAD_PATH)
    await expect(page.getByText('二行目')).toBeVisible()

    const css = await page.evaluate(async () => {
      const hrefs = [...document.styleSheets].map((s) => s.href).filter(Boolean)
      const texts = await Promise.all(hrefs.map((h) => fetch(h).then((r) => r.text())))
      return texts.join('\n')
    })
    // The scoped .res rule (any Svelte hash) must declare the callout suppression.
    expect(css).toMatch(/\.res[^{}]*\{[^}]*-webkit-touch-callout:\s*none/)
  })
})
