import { expect, test, vi } from 'vitest'

import { open, shown } from '../testing/page'
import { server } from '../testing/server'

const START = new Date('2026-01-01T00:00:00Z').getTime()

const FAVORITE = {
  server: 'egg',
  board: 'software',
  board_name: 'ソフトウェア',
  thread_id: '1000000001',
  title: '更新前',
  res_count: 10,
  read_res: 10,
  rating: 0,
  status: 'active',
}

test('visible favorites page reloads cached list every 60 seconds only', async () => {
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'], now: START + 1_000 })

  let listRequests = 0
  const api = server().on('/api/favorites', () => {
    listRequests += 1
    return [{ ...FAVORITE, title: listRequests === 1 ? '更新前' : '更新後' }]
  })

  open(api, '/')
  await shown('更新前')
  expect(listRequests).toBe(1)

  await vi.advanceTimersByTimeAsync(59_999)
  expect(listRequests).toBe(1)

  await vi.advanceTimersByTimeAsync(1)
  await shown('更新後')
  expect(listRequests).toBe(2)
  expect(api.calls(/\/api\/favorites\/(refresh|.+\/(reload|dat))$/)).toEqual([])
})
