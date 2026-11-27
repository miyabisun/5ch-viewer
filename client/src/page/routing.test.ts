import { screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, back, click, found, open, path, shown, texts, until } from '../testing/page'
import { server } from '../testing/server'

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

function mock() {
  return (
    server()
      .on('/api/favorites', [FAV])
      .on(/\/api\/favorites\/.+\/dat$/, datResponse)
      // Entry renders stored dat only (no reload). Kept as a defensive mock in case
      // the footer 更新 button is exercised; it never fires on open.
      .on(/\/api\/favorites\/.+\/reload$/, { res_count: 1, read_res: 0, status: 'active' })
  )
}

test('opening a thread pushes its URL', async () => {
  open(mock(), '/')
  expect(path()).toBe('/')

  await click(await found('.info'))
  await shown('本文1')
  expect(path()).toBe(THREAD_PATH)
})

test('browser back returns from thread to the list', async () => {
  open(mock(), '/')
  await click(await found('.info'))
  expect(path()).toBe(THREAD_PATH)
  await shown('本文1')

  await back()
  expect(path()).toBe('/')
  // The list is shown again; the thread body is gone.
  await until(() => expect($('.thread .info')).not.toBeNull())
  await until(() => expect(texts('本文1')).toHaveLength(0))
})

test('direct access to a thread URL opens the thread', async () => {
  open(mock(), THREAD_PATH)
  await shown('本文1')
  expect(path()).toBe(THREAD_PATH)
})

test('register tab updates the URL to /register', async () => {
  open(mock(), '/')
  await click(screen.getByTestId('tab-register'))
  expect(path()).toBe('/register')

  await click(screen.getByTestId('tab-favorites'))
  expect(path()).toBe('/')
})
