import { expect, test } from 'vitest'

import { $, $$, click, open, seen, shown, until } from '../testing/page'
import { server, type Asked } from '../testing/server'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'テストスレ',
  res_count: 30,
  read_res: 28,
  rating: 0,
  status: 'active',
}

const COUNT = 30

function datResponse() {
  return {
    title: FAV.title,
    res_count: COUNT,
    read_res: FAV.read_res,
    status: 'active',
    res: Array.from({ length: COUNT }, (_, i) => ({
      num: i + 1,
      name: '名無し',
      mail: '',
      date: `2025 ID:x${i}`,
      body: `本文${i + 1}`,
    })),
  }
}

// Unread badge decreases as the user reads (onprogress fires without the save's debounce).
test('reading the lower reses immediately reduces the unread badge in the list pane', async () => {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse())
    .on(/\/api\/favorites\/.+\/reload$/, {
      res_count: COUNT,
      read_res: FAV.read_res,
      status: 'active',
    })
    .on(/\/api\/favorites\/.+\/progress$/, ({ method }: Asked) =>
      method === 'GET' ? { read_res: FAV.read_res } : { ok: true },
    )
  open(api, '/')

  // Initially unread badge = 30 - 28 = 2 (ThreadRow badge in list pane).
  await click(await shown(FAV.title))
  await until(() => expect($('.list-pane .unread')?.textContent).toBe('2'))
  await shown('本文1')

  // Res 30 comes into sight in the detail pane: the badge drops to 0.
  await until(() => {
    seen($('.res[data-res="30"]') as HTMLElement)
    expect($$('.list-pane .unread')).toHaveLength(0)
  })
})
