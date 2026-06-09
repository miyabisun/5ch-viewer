import { test, expect } from '@playwright/test'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'テストスレ',
  res_count: 1,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

function datResponse() {
  return {
    title: FAV.title,
    res_count: 1,
    read_res: 0,
    status: 'active',
    res: [{ num: 1, name: '名無し', mail: '', date: '2025 ID:x', body: '本文1' }],
  }
}

function mock(page) {
  page.route('**/api/favorites', (route) => route.fulfill({ json: [FAV] }))
  page.route(/\/api\/favorites\/.+\/dat$/, (route) =>
    route.fulfill({ json: datResponse() }),
  )
}

test('opening a thread pushes its URL', async ({ page }) => {
  await mock(page)
  await page.goto('/')
  await expect(page).toHaveURL('/')

  await page.locator('.info').first().click()
  await expect(page.getByText('本文1')).toBeVisible()
  await expect(page).toHaveURL(THREAD_PATH)
})

test('browser back returns from thread to the list', async ({ page }) => {
  await mock(page)
  await page.goto('/')
  await page.locator('.info').first().click()
  await expect(page).toHaveURL(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()

  await page.goBack()
  await expect(page).toHaveURL('/')
  // The list (rating select) is shown again; the thread body is gone.
  await expect(page.locator('.thread .info')).toBeVisible()
  await expect(page.getByText('本文1')).toHaveCount(0)
})

test('direct access to a thread URL opens the thread', async ({ page }) => {
  await mock(page)
  await page.goto(THREAD_PATH)
  await expect(page.getByText('本文1')).toBeVisible()
  await expect(page).toHaveURL(THREAD_PATH)
})

test('register tab updates the URL to /register', async ({ page }) => {
  await mock(page)
  await page.goto('/')
  await page.getByTestId('tab-register').click()
  await expect(page).toHaveURL('/register')

  await page.getByTestId('tab-favorites').click()
  await expect(page).toHaveURL('/')
})
