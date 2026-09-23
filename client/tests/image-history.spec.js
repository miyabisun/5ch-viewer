import { test, expect, devices } from '@playwright/test'
import { mkdirSync } from 'node:fs'

const fav = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '123',
  title: '画像と戻る操作',
  res_count: 60,
  read_res: 35,
  rating: 0,
  status: 'active',
}
const path = '/egg/applism/123'
const viewer = (page) => page.locator('.viewer-bg')
const thumbnail = (page) => page.locator('.thread-body .res[data-res="35"] .thumb-btn').first()

async function mock(page) {
  let reads = 0
  let imagesAvailable = true
  await page.route('**/api/**', (route) => {
    const url = new URL(route.request().url())
    if (url.pathname === '/api/favorites')
      return route.fulfill({ json: [fav, { ...fav, thread_id: '456', title: '別のスレッド' }] })
    if (url.pathname.endsWith('/dat')) {
      reads++
      return route.fulfill({
        json: {
          ...fav,
          mosaic_urls: [],
          res: Array.from({ length: 60 }, (_, i) => ({
            num: i + 1,
            name: '名無し',
            mail: '',
            date: '2026/09/24',
            id: i === 34 ? 'PICTURES' : null,
            body:
              `本文${i + 1}<br>読み進めていた位置のテストです。` +
              (i === 34 && imagesAvailable
                ? '<br>https://example.com/one.jpg<br>https://example.com/two.jpg'
                : '') +
              (i === 35 ? '<br>&gt;&gt;35' : ''),
          })),
        },
      })
    }
    if (url.pathname.startsWith('/api/images/') && route.request().method() === 'GET')
      return route.fulfill({
        contentType: 'image/svg+xml',
        body: '<svg xmlns="http://www.w3.org/2000/svg" width="480" height="320"><rect width="480" height="320" fill="#196eaa"/><circle cx="240" cy="160" r="80" fill="#e2eaf0"/></svg>',
      })
    return route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } })
  })
  return {
    reads: () => reads,
    removeImages: () => {
      imagesAvailable = false
    },
  }
}

async function enter(page) {
  await page.goto('/')
  await page.locator('.info').filter({ hasText: fav.title }).click()
  await expect(page).toHaveURL(path)
  await thumbnail(page).scrollIntoViewIfNeeded()
}

async function capture(page, name) {
  if (!process.env.E2E_EVIDENCE_DIR) return
  mkdirSync(process.env.E2E_EVIDENCE_DIR, { recursive: true })
  await page.screenshot({ path: `${process.env.E2E_EVIDENCE_DIR}/${name}.png` })
}

const { defaultBrowserType, ...mobile } = devices['Pixel 5']
for (const [name, options] of [
  ['desktop', { viewport: { width: 1280, height: 800 }, colorScheme: 'light' }],
  ['mobile', { ...mobile, colorScheme: 'dark' }],
]) {
  test.describe(name, () => {
    test.use(options)

    test('Back closes only the image and Forward restores it without refetch or scroll reset', async ({
      page,
    }) => {
      const api = await mock(page)
      await enter(page)
      const body = await page.locator('.thread-body').elementHandle()
      const scroll = await body.evaluate((el) => el.scrollTop)
      const reads = api.reads()
      await thumbnail(page).click()
      await expect(viewer(page)).toBeVisible()
      await page.getByRole('button', { name: '次の画像' }).click()
      await expect(page.locator('.viewer-footer')).toHaveText('2 / 2')
      await page.keyboard.press('Control+ArrowLeft')
      await expect(page.locator('.viewer-footer')).toHaveText('2 / 2')
      if (name === 'mobile') {
        await page.getByRole('button', { name: '前の画像' }).click()
        const cdp = await page.context().newCDPSession(page)
        for (const [type, x] of [
          ['touchStart', 280],
          ['touchMove', 250],
          ['touchMove', 120],
          ['touchEnd', 120],
        ]) {
          await cdp.send('Input.dispatchTouchEvent', {
            type,
            touchPoints: type === 'touchEnd' ? [] : [{ x, y: 350 }],
          })
        }
        await cdp.detach()
        await expect(page.locator('.viewer-footer')).toHaveText('2 / 2')
      }
      await capture(page, `${name}-open`)
      await page.goBack()
      await expect(page).toHaveURL(path)
      await expect(viewer(page)).toHaveCount(0)
      expect(await body.evaluate((el) => el.isConnected)).toBe(true)
      expect(await body.evaluate((el) => el.scrollTop)).toBeCloseTo(scroll, 0)
      expect(api.reads()).toBe(reads)
      await capture(page, `${name}-closed`)
      await page.goForward()
      await expect(page.locator('.viewer-footer')).toHaveText('2 / 2')
      expect(api.reads()).toBe(reads)
      await page.goBack()
      await expect(viewer(page)).toHaveCount(0)
      await page.goBack()
      await expect(page).toHaveURL('/')
      await expect(page.locator('.thread-body')).toHaveCount(0)
    })

    test('close controls consume one entry, repeated close and immediate reopening do not skip the thread', async ({
      page,
    }) => {
      await mock(page)
      await enter(page)
      for (const action of ['button', 'background', 'escape', 'swipe', 'double']) {
        await thumbnail(page).click()
        await expect(viewer(page)).toBeVisible()
        if (action === 'button') await page.locator('.viewer-close').click()
        if (action === 'background') await viewer(page).click({ position: { x: 4, y: 4 } })
        if (action === 'escape') await page.keyboard.press('Escape')
        if (action === 'swipe') {
          const cdp = await page.context().newCDPSession(page)
          for (const [type, y] of [
            ['touchStart', 220],
            ['touchMove', 250],
            ['touchMove', 380],
            ['touchEnd', 380],
          ])
            await cdp.send('Input.dispatchTouchEvent', {
              type,
              touchPoints: type === 'touchEnd' ? [] : [{ x: 200, y }],
            })
          await cdp.detach()
        }
        if (action === 'double')
          await page.locator('.viewer-close').evaluate((el) => {
            el.click()
            el.click()
          })
        await expect(viewer(page)).toHaveCount(0)
        await expect(page).toHaveURL(path)
      }
      await page.goBack()
      await expect(page).toHaveURL('/')
    })

    test('image menu and popup images share the close history', async ({ page }) => {
      await mock(page)
      await enter(page)
      await thumbnail(page).click()
      if (name === 'mobile') {
        const cdp = await page.context().newCDPSession(page)
        await cdp.send('Input.dispatchTouchEvent', {
          type: 'touchStart',
          touchPoints: [{ x: 200, y: 350 }],
        })
        await expect(page.getByTestId('image-menu')).toBeVisible()
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] })
        await cdp.detach()
      } else await page.locator('.viewer-img').click({ button: 'right' })
      await expect(viewer(page)).toHaveCount(0)
      await expect(page.getByTestId('image-menu')).toBeVisible()
      await page.getByRole('button', { name: 'モザイクをかける' }).click()
      await thumbnail(page).click()
      await expect(page.locator('.viewer-img')).toHaveClass(/mosaic/)
      await page.goBack()
      await expect(viewer(page)).toHaveCount(0)
      for (const trigger of [
        '.thread-body .anchor[data-anchor="35"]',
        '.thread-body .id-badge[data-id="PICTURES"]',
      ]) {
        await page.locator(trigger).click()
        await page.locator('.modal .thumb-btn').first().click()
        await expect(viewer(page)).toBeVisible()
        await page.goBack()
        await expect(viewer(page)).toHaveCount(0)
        await expect(page.locator('.modal')).toBeVisible()
        await page.locator('.modal .thumb-btn').first().click()
        await page.keyboard.press('Escape')
        await expect(viewer(page)).toHaveCount(0)
        await expect(page.locator('.modal')).toBeVisible()
        await page.locator('.modal-close').click()
      }
      await page.goBack()
      await expect(page).toHaveURL('/')
    })

    test('direct URLs, reload and navigating away leave coherent entries', async ({ page }) => {
      const api = await mock(page)
      await page.goto('/register')
      await page.goto(path)
      await thumbnail(page).click()
      await page.getByRole('button', { name: '次の画像' }).click()
      await page.reload()
      await expect(page.locator('.viewer-footer')).toHaveText('2 / 2')
      await page.goBack()
      await expect(viewer(page)).toHaveCount(0)
      await expect(page).toHaveURL(path)
      await thumbnail(page).click()
      // Exercise app navigation while the component is being discarded (same path on both viewports).
      await page.getByTestId('tab-register').evaluate((el) => el.click())
      await expect(page).toHaveURL('/register')
      await page.goBack()
      await expect(page).toHaveURL(path)
      await expect(viewer(page)).toHaveCount(0)
      await thumbnail(page).click()
      await page
        .locator('.info')
        .filter({ hasText: '別のスレッド' })
        .evaluate((el) => el.click())
      await expect(page).toHaveURL('/egg/applism/456')
      await expect(viewer(page)).toHaveCount(0)
      await page.goBack()
      await expect(page).toHaveURL(path)
      await expect(viewer(page)).toHaveCount(0)
      await thumbnail(page).click()
      api.removeImages()
      await page.reload()
      await expect(viewer(page)).toHaveCount(0)
      await expect(page.locator('.thread-body .res')).toHaveCount(60)
      await expect.poll(() => page.evaluate(() => history.state?.imageViewer ?? null)).toBeNull()
      await page.goBack()
      await expect(page).toHaveURL('/register')
    })
  })
}
