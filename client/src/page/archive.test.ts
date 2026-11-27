import { screen, within } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, $$, click, found, open, path, rightClick, shown, until } from '../testing/page'
import { server } from '../testing/server'

const fav = (thread_id: string, title: string, res_count: number, read_res: number) => ({
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id,
  title,
  res_count,
  read_res,
  rating: 0,
  status: 'active',
})

const FAVS = [fav('1000000001', 'スレA', 10, 5), fav('1000000002', 'スレB', 20, 20)]

const ARCHIVES = [
  fav('2000000001', 'アーカイブA', 10, 8),
  fav('2000000002', 'アーカイブB', 5, 5),
  { ...fav('3000000001', 'アーカイブC', 3, 0), board: 'gekikara', board_name: 'テスト板' },
]

const base = (archives: unknown[] = []) =>
  server().on('/api/favorites', FAVS).on('/api/archives', archives)

/** Records the bodies of PATCH .../archived. */
function archiving(api: ReturnType<typeof base>) {
  const sent: unknown[] = []
  api.on(/\/api\/favorites\/.+\/archived$/, ({ body }: { body: unknown }) => {
    sent.push(body)
    return { ok: true }
  })
  return sent
}

const menu = () => $('.menu') as HTMLElement

// ── Tab navigation ──────────────────────────────────────────────────────────

test('archive tab is visible and switches to /archive', async () => {
  open(base(), '/')

  const tab = screen.getByTestId('tab-archive')
  await click(tab)
  expect(tab.classList).toContain('active')
  expect(path()).toBe('/archive')
})

test('archive tab is not active on favorites page', async () => {
  open(base(), '/')

  expect(screen.getByTestId('tab-favorites').classList).toContain('active')
  expect(screen.getByTestId('tab-archive').classList).not.toContain('active')
})

// ── Favorites menu: アーカイブ button ────────────────────────────────────────

test('favorites menu has アーカイブ button above 削除', async () => {
  open(base(), '/')

  await rightClick(await found('.thread'))
  await until(() => expect(menu()).not.toBeNull())

  const buttons = $$('button', menu())
  const archive = within(menu()).getByRole('button', { name: 'アーカイブ' })
  const remove = within(menu()).getByRole('button', { name: '削除' })
  expect(buttons.indexOf(archive)).toBeLessThan(buttons.indexOf(remove))
})

test('pressing アーカイブ sends PATCH .../archived with {archived:true}', async () => {
  const api = base()
  const sent = archiving(api)
  open(api, '/')

  await rightClick(await found('.thread'))
  await click(within(await found('.menu')).getByRole('button', { name: 'アーカイブ' }))

  await until(() => expect(sent).toHaveLength(1))
  expect(sent[0]).toEqual({ archived: true })

  // Menu closes after the action.
  await until(() => expect(menu()).toBeNull())
})

// ── Archive page: board accordion ───────────────────────────────────────────

test('archive page shows board accordion collapsed by default', async () => {
  open(base(ARCHIVES), '/archive')

  // Two distinct boards → two headers.
  await until(() => expect($$('.board-header')).toHaveLength(2))

  // No thread rows visible yet (accordion collapsed).
  expect($$('.thread')).toHaveLength(0)
})

test('clicking board header expands accordion and shows threads', async () => {
  open(base(ARCHIVES), '/archive')

  // Expand the first board (アプリ has 2 threads).
  await click(await found('.board-header'))
  await until(() => expect($$('.thread')).toHaveLength(2))

  // Collapse it again.
  await click($$('.board-header')[0])
  await until(() => expect($$('.thread')).toHaveLength(0))
})

test('board header shows board name and thread count', async () => {
  open(base(ARCHIVES), '/archive')

  await until(() => expect($$('.board-header')).toHaveLength(2))
  const headers = $$('.board-header')
  expect($('.board-name', headers[0])?.textContent).toBe('アプリ')
  expect($('.board-count', headers[0])?.textContent).toBe('(2)')
  expect($('.board-name', headers[1])?.textContent).toBe('テスト板')
  expect($('.board-count', headers[1])?.textContent).toBe('(1)')
})

test('unread badge shown for archive threads with unread posts', async () => {
  open(base(ARCHIVES), '/archive')

  // Expand アプリ: アーカイブA has 10-8=2 unread, アーカイブB has 0.
  await click(await found('.board-header'))

  await until(() => expect($$('.unread')).toHaveLength(1))
  expect($('.unread')?.textContent).toBe('2')
})

test('clicking archive thread row calls onopen (navigates to thread)', async () => {
  const api = base(ARCHIVES)
    .on(/\/api\/favorites\/.+\/dat$/, {
      title: 'アーカイブA',
      res_count: 10,
      read_res: 8,
      status: 'active',
      res: [{ num: 1, name: '名無し', mail: '', date: '2025 ID:x', body: 'アーカイブ本文' }],
    })
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 10, read_res: 8, status: 'active' })
  open(api, '/archive')

  await click(await found('.board-header'))
  await click(await found('.thread .info'))
  await shown('アーカイブ本文')
})

test('archive list アーカイブ解除 sends PATCH .../archived with {archived:false}', async () => {
  const api = base(ARCHIVES)
  const sent = archiving(api)
  open(api, '/archive')

  await click(await found('.board-header'))
  await rightClick(await found('.thread'))
  await until(() => expect(menu()).not.toBeNull())

  await click(screen.getByRole('button', { name: 'アーカイブ解除' }))
  await until(() => expect(sent).toHaveLength(1))
  expect(sent[0]).toEqual({ archived: false })
})

test('empty archive page shows placeholder message', async () => {
  open(base(), '/archive')

  await until(() => expect($('.empty')?.textContent).toBe('アーカイブはありません'))
})
