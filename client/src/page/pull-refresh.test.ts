import { screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, click, open, shown, until } from '../testing/page'
import { server, type Asked } from '../testing/server'

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

function datResponse(count = 40) {
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

const progress = ({ method }: Asked) => (method === 'GET' ? { read_res: 0 } : { ok: true })

test('refresh button click fires a GET reload and adds new reses as unread', async () => {
  const INITIAL_COUNT = 40
  const ADDED_COUNT = 2
  let datCallCount = 0
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, () =>
      ++datCallCount === 1 ? datResponse(INITIAL_COUNT) : datResponse(INITIAL_COUNT + ADDED_COUNT),
    )
    .on(/\/api\/favorites\/.+\/reload$/, {
      res_count: INITIAL_COUNT + ADDED_COUNT,
      read_res: 0,
      status: 'active',
    })
    .on(/\/api\/favorites\/.+\/progress$/, progress)
  open(api, THREAD_PATH)
  await shown(`本文${INITIAL_COUNT}`)

  await click(screen.getByRole('button', { name: '更新' }))

  await shown(`本文${INITIAL_COUNT + ADDED_COUNT}`)
  expect(api.calls(/\/reload$/, 'GET')).toHaveLength(1)

  // All reses (initial + added) are unread: read_res = 0, nothing read yet.
  for (let i = 1; i <= INITIAL_COUNT + ADDED_COUNT; i++) {
    expect($(`.res[data-res="${i}"]`)?.classList).toContain('unread')
  }
})

test('no double-fire: clicking refresh again while in flight is ignored', async () => {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse(40))
    .on(/\/api\/favorites\/.+\/reload$/, async () => {
      await new Promise((done) => setTimeout(done, 800))
      return { res_count: 40, read_res: 0, status: 'active' }
    })
  open(api, THREAD_PATH)
  await shown('本文1')

  const button = screen.getByRole('button', { name: '更新' }) as HTMLButtonElement
  await click(button)
  // While the reload is in flight the button is disabled.
  await until(() => expect(button.disabled).toBe(true))

  // A second click (forced through the disabled state) must not start another reload.
  await click(button)

  await new Promise((done) => setTimeout(done, 1000))
  expect(api.calls(/\/reload$/)).toHaveLength(1)
})

test('opening a thread without scrolling shows unread bar on unread reses (regression)', async () => {
  const COUNT = 5
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse(COUNT))
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: COUNT, read_res: 0, status: 'active' })
    .on(/\/api\/favorites\/.+\/progress$/, progress)
  open(api, THREAD_PATH)
  await shown('本文1')

  for (let i = 1; i <= COUNT; i++) {
    expect($(`.res[data-res="${i}"]`)?.classList).toContain('unread')
  }
})
