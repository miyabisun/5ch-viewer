import { screen, within } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, click, found, open, path, shown, texts, until } from '../testing/page'
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

function datResponse(count: number) {
  return {
    title: FAV.title,
    res_count: count,
    read_res: 0,
    status: 'active',
    res: Array.from({ length: count }, (_, i) => ({
      num: i + 1,
      name: '名無し',
      mail: '',
      date: `2025 ID:x${i}`,
      body: `本文${i + 1}`,
    })),
  }
}

// Entry (ChMate model): opening a thread renders the stored dat with no 5ch access.
test('opening a thread renders stored dat with zero reload requests', async () => {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse(1))
    .on(/\/api\/favorites\/.+\/reload$/, async () => {
      await new Promise((done) => setTimeout(done, 1000))
      return { res_count: 2, read_res: 0, status: 'active', updated: true }
    })
  open(api, '/')

  await click(await found('.info'))
  await shown('本文1')
  expect(api.calls(/\/reload$/)).toHaveLength(0)

  // No back button in the thread header.
  const detail = $('.detail-pane') as HTMLElement
  expect(within(detail).queryAllByRole('button', { name: /戻る/ })).toHaveLength(0)
})

// The footer refresh button runs the GET reload and the grown dat renders.
test('footer refresh grows the drifted dat (111 -> 117) via GET reload', async () => {
  let count = 111
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, () => datResponse(count))
    .on(/\/api\/favorites\/.+\/reload$/, () => {
      count = 117 // the reload grew the dat
      return { res_count: 117, read_res: 0, status: 'active', updated: true }
    })
  open(api, '/')
  await click(await found('.info'))

  await shown('本文111')
  expect(texts('本文117')).toHaveLength(0)

  const detail = $('.detail-pane') as HTMLElement
  await click(within(detail).getByRole('button', { name: '更新' }))
  await shown('本文117')
  expect(api.calls(/\/reload$/).map((asked) => asked.method)).toEqual(['GET'])
})

test('favorites tab returns from a thread to the list', async () => {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse(1))
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 1, read_res: 0, status: 'active' })
  open(api, '/')
  await click(await found('.info'))
  await shown('本文1')

  await click(screen.getByTestId('tab-favorites'))
  expect(path()).toBe('/')
  await until(() => expect($('.thread .info')).not.toBeNull())
  await until(() => expect(texts('本文1')).toHaveLength(0))
})
