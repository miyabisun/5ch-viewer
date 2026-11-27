import { screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, click, found, open, rightClick, shown, until } from '../testing/page'
import { server } from '../testing/server'

// Source (dead) thread and its successor.
const SRC = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127100',
  title: 'ブルアカ Part5862',
  res_count: 1002,
  read_res: 0,
  rating: 3,
  status: 'dead',
}
const NEXT = {
  ...SRC,
  thread_id: '1771127101',
  title: 'ブルアカ Part5863',
  res_count: 5,
  status: 'active',
}

test('menu has "次スレを検索"; on found it fires find-next once and the new thread appears', async () => {
  let favState = [SRC]
  let findNextHits = 0
  const api = server()
    .on('/api/favorites', () => favState)
    .on(/\/find-next$/, () => {
      findNextHits += 1
      // After a successful find-next the successor is registered server-side.
      favState = [SRC, NEXT]
      return {
        found: true,
        server: NEXT.server,
        board: NEXT.board,
        thread_id: NEXT.thread_id,
        title: NEXT.title,
      }
    })
  open(api, '/')

  // Open the source thread's action menu (right-click).
  await rightClick(await found('.thread.rate-3'))
  await until(() => expect($('.menu')).not.toBeNull())

  // The "次スレを検索" button is present in the menu.
  await click(screen.getByRole('button', { name: '次スレを検索' }))

  // find-next fired exactly once.
  await until(() => expect(findNextHits).toBe(1))

  // The list re-reads and now shows the successor thread.
  await shown('ブルアカ Part5863')
})

test('on found:false the menu shows inline feedback and stays open', async () => {
  const api = server()
    .on('/api/favorites', [SRC])
    .on(/\/find-next$/, { found: false })
  open(api, '/')

  await rightClick(await found('.thread.rate-3'))
  await click(screen.getByRole('button', { name: '次スレを検索' }))

  await until(() =>
    expect(screen.getByTestId('find-next-status').textContent).toBe('次スレは見つかりませんでした'),
  )
  // The menu stays open so the user sees the result.
  expect($('.menu')).not.toBeNull()
})
