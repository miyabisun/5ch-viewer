import { screen, within } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import {
  $,
  $$,
  click,
  found,
  longPress,
  open,
  rightClick,
  shown,
  texts,
  until,
} from '../testing/page'
import { server, type Asked } from '../testing/server'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'ワッチョイメニューテストスレ',
  res_count: 3,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

const post = (num: number, name: string, date: string, body: string) => ({
  num,
  name,
  mail: '',
  date,
  body,
  id: null,
})

// Posts 1 and 3 share the wacchoi 7bb6-83IP; post 2 is aaaa-BBBB.
function datResponse() {
  return {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      post(
        1,
        'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        '2025/01/01(水) 00:00:00.00',
        '本文1',
      ),
      post(2, '名無し</b>(ﾜｯﾁｮｲ aaaa-BBBB [::1])<b>', '2025/01/01(水) 00:01:00.00', '本文2'),
      post(
        3,
        'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        '2025/01/01(水) 00:02:00.00',
        '本文3',
      ),
    ],
  }
}

const both = ({ method }: Asked) => (method === 'GET' ? [] : { ok: true })

function setupRoutes({
  ngWacchoi = [] as unknown,
  dat = datResponse() as unknown,
  search = [] as unknown,
} = {}) {
  return server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, dat)
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 3, read_res: 0, status: 'active' })
    .on('/api/ng-ids', both)
    .on(/\/api\/ng-words(\?|$)/, both)
    .on(/\/api\/ng-wacchoi(\?|$)/, { ok: true })
    .on(/\/api\/ng-wacchoi(\?|$)/, ngWacchoi, 'GET')
    .on(/\/api\/boards\/.+\/wacchoi-search/, search)
}

/** Records the bodies of POST /api/ng-wacchoi. */
function adding(api: ReturnType<typeof setupRoutes>) {
  const added: any[] = []
  api.on(
    /\/api\/ng-wacchoi(\?|$)/,
    ({ body }: Asked) => {
      added.push(body)
      return { ok: true }
    },
    'POST',
  )
  return added
}

const testid = (id: string) => $(`[data-testid="${id}"]`)
const ngHeaders = (text: string) => $$('del.ng').filter((el) => el.textContent?.includes(text))
const SCOPED_NG = {
  suffix: '83IP',
  board: 'applism',
  week_key: '2024/12/29',
  wacchoi: '7bb6-83IP',
  created_at: 0,
}

// ── Right-click → wacchoi-menu ──────────────────────────────────────────────

test('right-click on wacchoi badge opens wacchoi-menu modal', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')

  await rightClick(await found('.wacchoi-badge'))
  await found('[data-testid="wacchoi-menu"]')
  expect(testid('reply-menu')).toBeNull()
})

test('long-press on wacchoi badge opens only the wacchoi menu', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')

  await longPress(await found('.wacchoi-badge'))
  await found('[data-testid="wacchoi-menu"]')
  expect(testid('reply-menu')).toBeNull()
})

test('wacchoi tap still works after closing a reply menu opened from the res header', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')

  const num = await found('.res .num')
  await longPress(num)
  num.dispatchEvent(new MouseEvent('click', { bubbles: true }))
  await found('[data-testid="reply-menu"]')
  await click(screen.getByRole('button', { name: '閉じる' }))

  await click($('.wacchoi-badge') as HTMLElement)
  await found('[data-testid="wacchoi-list"]')
})

test('wacchoi-menu modal header shows the wacchoi token', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')
  await rightClick(await found('.wacchoi-badge'))

  await until(() => expect($('.modal')?.textContent).toContain('ﾜｯﾁｮｲ:7bb6-83IP'))
})

test('wacchoi-menu contains NG button, copy button, and search button', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')
  await rightClick(await found('.wacchoi-badge'))
  await found('[data-testid="wacchoi-menu"]')

  expect(screen.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'コピー' })).toBeTruthy()
  expect(screen.getByRole('button', { name: '取得済みスレから検索' })).toBeTruthy()
})

test('copy writes "ワッチョイ 7bb6-83IP" to clipboard', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')
  await rightClick(await found('.wacchoi-badge'))
  await click(screen.getByRole('button', { name: 'コピー' }))

  await until(() =>
    expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('ワッチョイ 7bb6-83IP'),
  )
})

test('wacchoi-menu closes via × button', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')
  await rightClick(await found('.wacchoi-badge'))
  await found('[data-testid="wacchoi-menu"]')

  await click(screen.getByRole('button', { name: '閉じる' }))
  await until(() => expect(testid('wacchoi-menu')).toBeNull())
})

test('left-click on wacchoi badge opens wacchoi-list (not wacchoi-menu)', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')

  await click(await found('.wacchoi-badge'))
  await found('[data-testid="wacchoi-list"]')
  expect(testid('wacchoi-menu')).toBeNull()
})

// ── NG wacchoi add ──────────────────────────────────────────────────────────

test('NGﾜｯﾁｮｲに追加 calls POST /api/ng-wacchoi with correct suffix, board, week_key', async () => {
  const api = setupRoutes()
  const added = adding(api)
  open(api, THREAD_PATH)
  await shown('本文1')

  await rightClick(await found('.wacchoi-badge'))
  await found('[data-testid="wacchoi-menu"]')
  await click(screen.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }))

  await until(() => expect(added).toHaveLength(1))
  // res 1 is 2025/01/01 (Wed): its week starts on Sunday 2024/12/29.
  expect(added[0].suffix).toBe('83IP')
  expect(added[0].board).toBe('applism')
  expect(added[0].week_key).toBe('2024/12/29')
  expect(added[0].wacchoi).toBe('7bb6-83IP')
})

test('NGﾜｯﾁｮｲ追加後はワッチョイメニューに削除ボタンを出さない', async () => {
  let reads = 0
  // First GET (initial load) returns []; after the add, GET returns the entry.
  const api = setupRoutes({ ngWacchoi: () => (++reads > 1 ? [SCOPED_NG] : []) })
  open(api, THREAD_PATH)
  await shown('本文1')

  await rightClick(await found('.wacchoi-badge'))
  await click(screen.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }))

  await until(() =>
    expect(ngHeaders('1 NGワッチョイ')[0]?.textContent?.trim()).toBe('1 NGワッチョイ'),
  )
  expect($('.wacchoi-badge[data-wacchoi="7bb6-83IP"]')).toBeNull()
  expect(screen.queryAllByRole('button', { name: 'NGﾜｯﾁｮｲから削除' })).toHaveLength(0)
})

test('NG wacchoi post menu removes the matching scoped entry', async () => {
  const api = setupRoutes({ ngWacchoi: [SCOPED_NG] })
  open(api, THREAD_PATH)

  await until(() =>
    expect(ngHeaders('1 NGワッチョイ')[0]?.textContent?.trim()).toBe('1 NGワッチョイ'),
  )
  await rightClick(ngHeaders('1 NGワッチョイ')[0])
  await found('[data-testid="ng-menu"]')
  await click(screen.getByRole('button', { name: 'NGワッチョイから削除' }))

  await until(() => expect(api.calls(/\/api\/ng-wacchoi(\?|$)/, 'DELETE')).toHaveLength(1))
  const [removed] = api.calls(/\/api\/ng-wacchoi(\?|$)/, 'DELETE')
  expect(Object.fromEntries(new URLSearchParams(removed.search))).toEqual({
    suffix: '83IP',
    board: 'applism',
    week_key: '2024/12/29',
  })
})

// ── NG scope (suffix + board + week_key) ────────────────────────────────────

function datForNgScope() {
  return {
    title: FAV.title,
    res_count: 5,
    read_res: 0,
    status: 'active',
    res: [
      post(
        1,
        'A</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>',
        '2025/01/01(水) 00:00:00.00',
        '同板同週同末尾A',
      ),
      post(
        2,
        'B</b>(ﾜｯﾁｮｲ cccc-83IP [::3])<b>',
        '2025/01/01(水) 00:01:00.00',
        '同板同週同末尾B(前半違い)',
      ),
      post(3, 'C</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>', '2025/01/04(土) 23:59:59.99', '同週土曜同末尾'),
      post(4, 'D</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>', '2025/01/05(日) 00:00:00.00', '翌週日曜同末尾'),
      post(5, 'E</b>(ﾜｯﾁｮｲ 7bb6-ZZZZ [2400::])<b>', '2025/01/01(水) 00:02:00.00', '同板同週別末尾'),
    ],
  }
}

test('NG scope: same suffix + same board + same week are hidden (different prefix is also NG)', async () => {
  open(setupRoutes({ ngWacchoi: [SCOPED_NG], dat: datForNgScope() }), THREAD_PATH)

  await until(() => expect($$('del.ng')).toHaveLength(3))
  expect(texts('同板同週同末尾A')).toHaveLength(0)
  expect(texts('同板同週同末尾B(前半違い)')).toHaveLength(0)
  expect(texts('同週土曜同末尾')).toHaveLength(0)
})

test('NG scope: different week with same suffix is NOT hidden (誤爆しない)', async () => {
  open(setupRoutes({ ngWacchoi: [SCOPED_NG], dat: datForNgScope() }), THREAD_PATH)

  await shown('翌週日曜同末尾')
  await shown('同板同週別末尾')
})

// ── Search ──────────────────────────────────────────────────────────────────

test('取得済みスレから検索 shows wacchoi-search-result modal', async () => {
  const search = [
    {
      thread_id: '9999999999',
      title: '別スレ',
      res: [post(5, 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>', '2025/01/01', '検索ヒット')],
    },
  ]
  open(setupRoutes({ search }), THREAD_PATH)
  await shown('本文1')

  await rightClick(await found('.wacchoi-badge'))
  await click(screen.getByRole('button', { name: '取得済みスレから検索' }))

  await found('[data-testid="wacchoi-search-result"]')
  await shown('検索ヒット')
})

test('取得済みスレから検索 shows 該当なし when no results', async () => {
  open(setupRoutes(), THREAD_PATH)
  await shown('本文1')

  await rightClick(await found('.wacchoi-badge'))
  await click(screen.getByRole('button', { name: '取得済みスレから検索' }))

  await found('[data-testid="wacchoi-search-result"]')
  await shown('該当なし')
})

// ── Week boundary (Sunday 00:00 JST) ────────────────────────────────────────

async function addNgAt(date: string) {
  const dat = {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [1, 2].map((num) =>
      post(num, 'A</b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>', date, `境界レス${num}`),
    ),
  }
  const api = setupRoutes({ dat })
  const added = adding(api)
  open(api, THREAD_PATH)
  await shown('境界レス1')
  await rightClick(await found('.wacchoi-badge'))
  await click(screen.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }))
  await until(() => expect(added).toHaveLength(1))
  return added[0]
}

test('week_key: 2025/01/05(日) 00:00 starts a new week (week_key 2025/01/05)', async () => {
  expect((await addNgAt('2025/01/05(日) 00:00:00.00')).week_key).toBe('2025/01/05')
})

test('week_key: 2025/01/04(土) 23:59 belongs to the week of 2024/12/29', async () => {
  expect((await addNgAt('2025/01/04(土) 23:59:59.99')).week_key).toBe('2024/12/29')
})

// ── Suffix grouping ─────────────────────────────────────────────────────────

function datForSuffixGroup() {
  const res = [
    ['iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>', '自宅回線'],
    ['iPhone774G </b>(ﾜｯﾁｮｲ cccc-83IP [::3])<b>', '別回線'],
    ['iPhone774G </b>(ｱｳｱｳｳｰ Sa1b-83IP)<b>', 'モバイル回線'],
    ['名無し</b>(ﾜｯﾁｮｲ 7bb6-ZZZZ [2400::])<b>', '別人'],
    ['名無し</b>(ﾜｯﾁｮｲ 86fb-/dMI)<b>', 'スラッシュA'],
    ['名無し</b>(ﾜｯﾁｮｲ 86b0-/dMI)<b>', 'スラッシュB'],
  ]
  return {
    title: FAV.title,
    res_count: res.length,
    read_res: 0,
    status: 'active',
    res: res.map(([name, body], i) => post(i + 1, name, `2025/01/01(水) 00:0${i}:00.00`, body)),
  }
}

async function gotoSuffixGroup(opts: { ngWacchoi?: unknown } = {}) {
  const api = setupRoutes({ ...opts, dat: datForSuffixGroup() })
  open(api, THREAD_PATH)
  await shown('自宅回線')
  return api
}

test('tapping a badge lists every res with the same suffix, whatever the prefix', async () => {
  await gotoSuffixGroup()

  await click(await found('.wacchoi-badge[data-wacchoi="cccc-83IP"]'))
  const list = await found('[data-testid="wacchoi-list"]')
  expect($('.modal')?.textContent).toContain('ﾜｯﾁｮｲ末尾:83IP（3件）')
  expect($$('.res', list)).toHaveLength(3)
  expect(list.textContent).toContain('自宅回線')
  expect(list.textContent).toContain('別回線')
  expect(list.textContent).toContain('モバイル回線')
  expect(list.textContent).not.toContain('別人')
})

test('reses sharing a suffix take the same heat colour; a lone suffix takes none', async () => {
  await gotoSuffixGroup()

  const nameOf = (token: string) =>
    $(`.wacchoi-badge[data-wacchoi="${token}"]`)?.closest('.name') as HTMLElement
  expect(nameOf('7bb6-83IP').className).toMatch(/id-l2/)
  expect(nameOf('cccc-83IP').className).toMatch(/id-l2/)
  expect(nameOf('Sa1b-83IP').className).toMatch(/id-l2/)
  expect(nameOf('7bb6-ZZZZ').className).not.toMatch(/id-l/)
})

test('a suffix containing "/" becomes a badge, is listed, and is sent to NG as it is', async () => {
  const api = await gotoSuffixGroup()
  const added = adding(api)

  const badge = await found('.wacchoi-badge[data-wacchoi="86fb-/dMI"]')
  expect(badge.textContent?.trim()).toBe('86fb-/dMI')
  await click(badge)
  await until(() => expect($('.modal')?.textContent).toContain('ﾜｯﾁｮｲ末尾:/dMI（2件）'))
  await click(screen.getByRole('button', { name: '閉じる' }))

  await rightClick($('.wacchoi-badge[data-wacchoi="86fb-/dMI"]') as HTMLElement)
  await click(screen.getByRole('button', { name: 'NGﾜｯﾁｮｲに追加' }))
  await until(() => expect(added).toHaveLength(1))
  expect(added[0]).toEqual({
    suffix: '/dMI',
    board: 'applism',
    week_key: '2024/12/29',
    wacchoi: '86fb-/dMI',
  })
})

test('an NG entry for a "/" suffix hides both prefixes and can be removed', async () => {
  const entry = { ...SCOPED_NG, suffix: '/dMI', wacchoi: '86fb-/dMI' }
  const api = await gotoSuffixGroup({ ngWacchoi: [entry] })

  await until(() => expect(ngHeaders('NGワッチョイ')).toHaveLength(2))
  expect(texts('スラッシュA')).toHaveLength(0)
  expect(texts('スラッシュB')).toHaveLength(0)

  await rightClick(ngHeaders('NGワッチョイ')[0])
  await click(screen.getByRole('button', { name: 'NGワッチョイから削除' }))
  await until(() => expect(api.calls(/\/api\/ng-wacchoi(\?|$)/, 'DELETE')).toHaveLength(1))
  const [removed] = api.calls(/\/api\/ng-wacchoi(\?|$)/, 'DELETE')
  expect(Object.fromEntries(new URLSearchParams(removed.search))).toEqual({
    suffix: '/dMI',
    board: 'applism',
    week_key: '2024/12/29',
  })
})
