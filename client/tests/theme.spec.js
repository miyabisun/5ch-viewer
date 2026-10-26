import { test, expect } from '@playwright/test'

// Washi / Sumi surface (--surface in App.svelte). In a browser tab theme-color must
// carry the page background of the active theme, not a default or a stale value.
const SURFACE = { light: 'rgb(250, 250, 250)', dark: 'rgb(25, 25, 25)' }

// The theme-color Chrome applies: the first meta[name=theme-color] in tree order
// whose media matches (HTML spec; Blink Document::ThemeColor).
function appliedThemeColor(page) {
  return page.evaluate(
    () =>
      [...document.querySelectorAll('meta[name="theme-color"]')].find(
        (meta) => !meta.media || matchMedia(meta.media).matches,
      )?.content,
  )
}

// Normalise a CSS colour to the computed rgb() form SURFACE uses.
function rgb(page, color) {
  return page.evaluate((value) => {
    const probe = document.createElement('i')
    probe.style.color = value
    document.body.append(probe)
    const computed = getComputedStyle(probe).color
    probe.remove()
    return computed
  }, color)
}

async function expectThemeColor(page, theme) {
  await expect.poll(() => appliedThemeColor(page)).toBe(SURFACE[theme])
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(
    SURFACE[theme],
  )
}

// In an installed app the standalone theme-color must come first, so it wins over
// the page mirror there, and carry the Sumi backdrop the manifest declares.
async function expectInstalledAppThemeColor(page) {
  const first = page.locator('meta[name="theme-color"]').first()
  await expect(first).toHaveAttribute('media', '(display-mode: standalone)')
  expect(await rgb(page, await first.getAttribute('content'))).toBe(SURFACE.dark)
}

test('theme toggle flips data-theme, persists, and survives reload', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  // Force a known OS default (light) so the toggle outcome is deterministic.
  await page.emulateMedia({ colorScheme: 'light' })
  await page.goto('/')

  const html = page.locator('html')
  await expect(html).toHaveAttribute('data-theme', 'light')
  await expectThemeColor(page, 'light')

  // Toggle to dark: attribute changes and is saved to localStorage.
  await page.getByTestId('theme-toggle').click()
  await expect(html).toHaveAttribute('data-theme', 'dark')
  await expectThemeColor(page, 'dark')
  expect(await page.evaluate(() => localStorage.getItem('5ch-theme'))).toBe('dark')

  // Reload: stored override is restored (independent of OS preference).
  await page.reload()
  await expect(html).toHaveAttribute('data-theme', 'dark')
  await expectThemeColor(page, 'dark')

  // Toggle back to light persists too.
  await page.getByTestId('theme-toggle').click()
  await expect(html).toHaveAttribute('data-theme', 'light')
  await expectThemeColor(page, 'light')
  expect(await page.evaluate(() => localStorage.getItem('5ch-theme'))).toBe('light')
})

test('theme follows OS preference when no override stored', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.emulateMedia({ colorScheme: 'dark' })
  await page.goto('/')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  await expectThemeColor(page, 'dark')

  // A light override on a dark OS must not keep the dark theme-color.
  await page.getByTestId('theme-toggle').click()
  await expectThemeColor(page, 'light')
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light')
  await expectThemeColor(page, 'light')
})

test('image viewer darkens theme-color while open and restores it on close', async ({ page }) => {
  const fav = {
    server: 'egg',
    board: 'b',
    thread_id: '1',
    title: '画像',
    res_count: 1,
    read_res: 0,
  }
  await page.route('**/api/**', (route) => {
    const url = new URL(route.request().url())
    if (url.pathname === '/api/favorites') return route.fulfill({ json: [fav] })
    if (url.pathname.endsWith('/dat'))
      return route.fulfill({
        json: {
          ...fav,
          mosaic_urls: [],
          res: [
            {
              num: 1,
              name: '名無し',
              mail: '',
              date: '',
              id: null,
              body: 'https://example.com/a.jpg',
            },
          ],
        },
      })
    if (url.pathname.startsWith('/api/images/'))
      return route.fulfill({
        contentType: 'image/svg+xml',
        body: '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"/>',
      })
    return route.fulfill({ json: route.request().method() === 'GET' ? [] : { ok: true } })
  })
  await page.emulateMedia({ colorScheme: 'light' })
  await page.goto('/egg/b/1')
  await expectThemeColor(page, 'light')

  // The near-black backdrop sits behind the system buttons, so they must turn light.
  await page.locator('.thumb-btn').first().click()
  await expect(page.locator('.viewer-bg')).toBeVisible()
  await expect.poll(() => appliedThemeColor(page)).toBe('rgb(20, 20, 20)')
  await expectInstalledAppThemeColor(page)

  await page.locator('.viewer-close').click()
  await expect(page.locator('.viewer-bg')).toBeHidden()
  await expectThemeColor(page, 'light')
})

// Android Chrome 153-155 draws an installed app's system buttons over the window
// background, fixed at install from the manifest background_color (#F8F9FA without
// one), and picks their contrast from theme-color. Both must agree whatever the page
// theme. Headless Chromium cannot enter display-mode: standalone, so this checks what
// Chrome reads there: the manifest and the standalone theme-color.
test('installed app keeps theme-color on the manifest backdrop', async ({ page }) => {
  await page.route('**/api/favorites', (route) => route.fulfill({ json: [] }))
  await page.route('**/api/favorites/refresh', (route) =>
    route.fulfill({ json: { ok: true, boards: 0 } }),
  )
  await page.emulateMedia({ colorScheme: 'light' })
  await page.goto('/')

  // 5ch.sis.jp sits behind Cloudflare Access; a manifest fetch omits cookies by default.
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute(
    'crossorigin',
    'use-credentials',
  )
  const cdp = await page.context().newCDPSession(page)
  const { errors, data } = await cdp.send('Page.getAppManifest')
  expect(errors).toEqual([])
  const manifest = JSON.parse(data)
  expect(manifest.display).toBe('standalone')
  expect(await rgb(page, manifest.background_color)).toBe(SURFACE.dark)
  expect(await rgb(page, manifest.theme_color)).toBe(SURFACE.dark)

  await expectInstalledAppThemeColor(page)
  await page.getByTestId('theme-toggle').click()
  await expectThemeColor(page, 'dark')
  await expectInstalledAppThemeColor(page)
})
