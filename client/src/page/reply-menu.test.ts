import { fireEvent, screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, click, found, longPress, open, rightClick, shown, until } from '../testing/page'
import { server, type Asked } from '../testing/server'

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

const both = ({ method }: Asked) => (method === 'GET' ? [] : { ok: true })

function serve() {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse())
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 2, read_res: 0, status: 'active' })
    .on('/api/ng-ids', both)
    .on(/\/api\/ng-words(\?|$)/, both)
    .on('/api/ng-wacchoi', both)
  open(api, THREAD_PATH)
  return api
}

const testid = (id: string) => $(`[data-testid="${id}"]`)

test('desktop: right-click on .body opens the reply menu', async () => {
  serve()
  await shown(/二行目/)

  await rightClick(await found('.res[data-res="1"] .body'))
  await found('[data-testid="reply-menu"]')
})

test('desktop: right-click on the res header opens the reply menu', async () => {
  serve()
  await shown(/二行目/)

  await rightClick(await found('.res .num'))
  await found('[data-testid="reply-menu"]')
})

test('desktop: right-click on image-strip whitespace opens the reply menu', async () => {
  serve()
  await fireEvent.contextMenu(await found('.thumb-strip'))
  await found('[data-testid="reply-menu"]')
})

test('desktop: an image context menu does not bubble into the res menu', async () => {
  serve()
  await rightClick(await found('.thumb-btn'))
  await found('[data-testid="image-menu"]')
  expect(testid('reply-menu')).toBeNull()
})

test('reply menu keeps the 返信する action and opens the post modal', async () => {
  serve()
  await shown(/二行目/)

  await rightClick(await found('.res[data-res="1"] .body'))
  await found('[data-testid="reply-menu"]')

  await click(screen.getByRole('button', { name: '返信する' }))
  await until(() => expect(($('.post-textarea') as HTMLTextAreaElement)?.value).toBe('>>1\n'))
})

test('本文をコピー writes the body (with newline) to the clipboard', async () => {
  serve()
  await shown(/二行目/)

  await rightClick(await found('.res[data-res="1"] .body'))
  await click(screen.getByRole('button', { name: '本文をコピー' }))

  await until(() =>
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('一行目\n二行目'),
  )
})

test('mobile: long-press on the res header opens the reply menu', async () => {
  serve()
  await shown(/二行目/)

  await longPress(await found('.res .num'))
  await found('[data-testid="reply-menu"]')
})

test('mobile: long-press on image-strip whitespace opens the reply menu', async () => {
  serve()
  await longPress(await found('.thumb-strip'))
  await found('[data-testid="reply-menu"]')
})

test('mobile: long-press on an image opens only the image menu', async () => {
  serve()
  await longPress(await found('.thumb-btn'))
  await found('[data-testid="image-menu"]')
  expect(testid('reply-menu')).toBeNull()
})

test('mobile: long-press on an ID opens only the ID menu', async () => {
  serve()
  await longPress(await found('.id-badge'))
  await found('[data-testid="id-menu"]')
  expect(testid('reply-menu')).toBeNull()
})
