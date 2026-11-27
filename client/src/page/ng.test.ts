import { screen, within } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, $$, click, found, open, rightClick, shown, texts, until } from '../testing/page'
import { server } from '../testing/server'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'NGテストスレ',
  res_count: 3,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

const res = (
  num: number,
  id: string,
  body: string,
  date = `2025/01/01(水) 00:0${num - 1}:00.00`,
) => ({
  num,
  name: '名無し',
  mail: '',
  date: `${date} ID:${id}`,
  body,
  id,
})

// Posts 1 and 3 share ID "target"; post 2 is "other".
function datResponse() {
  return {
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [res(1, 'target', '本文1'), res(2, 'other', '本文2'), res(3, 'target', '本文3')],
  }
}

// NG IDs are board-scoped: { server, board, ng_id } (never global).
function scopedNgId(ng_id: string) {
  return { server: FAV.server, board: FAV.board, ng_id, created_at: 0 }
}

function serve({
  ngIds = [] as unknown[] | (() => unknown),
  searchResult = [] as unknown[],
  dat = datResponse() as unknown,
} = {}) {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, dat)
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 3, read_res: 0, status: 'active' })
    .on(/\/api\/ng-ids(\?|$)/, { ok: true })
    .on(/\/api\/ng-ids(\?|$)/, ngIds, 'GET')
    .on(/\/api\/ng-words(\?|$)/, { ok: true })
    .on(/\/api\/ng-words(\?|$)/, [], 'GET')
    .on(/\/api\/boards\/.+\/id-search/, searchResult)
  open(api, THREAD_PATH)
  return api
}

const testid = (id: string) => $(`[data-testid="${id}"]`)
const ngHeader = (text: string) =>
  $$('del.ng').find((el) => el.textContent?.includes(text)) as HTMLElement

// ID badge left-click: opens id-list modal (all reses of that ID).
test('left-click on ID badge opens id-list modal with all reses of that ID', async () => {
  serve()
  await shown('本文1')

  // The first res (num 1) has ID "target" → left-click opens id-list.
  await click(await found('.resid'))

  const modal = await found('[data-testid="id-list"]')
  // Both reses with ID "target" (num 1 and num 3) are shown; "other" is not.
  expect(within(modal).getByText('本文1')).toBeTruthy()
  expect(within(modal).getByText('本文3')).toBeTruthy()
  expect(within(modal).queryAllByText('本文2')).toHaveLength(0)
})

test('id-list modal header shows ID and count', async () => {
  serve()
  await shown('本文1')
  await click(await found('.resid'))

  // Header must include the ID and count: "ID:target（2件）"
  await until(() => expect($('.modal')?.textContent).toContain('ID:target（2件）'))
})

test('left-click opens id-list; right-click opens id-menu (both routes distinct)', async () => {
  serve()
  await shown('本文1')

  // Left-click → id-list
  await click(await found('.resid'))
  await found('[data-testid="id-list"]')
  expect(testid('id-menu')).toBeNull()

  // Close via the top-right × (aria-label 閉じる).
  await click(screen.getByRole('button', { name: '閉じる' }))
  await until(() => expect(testid('id-list')).toBeNull())

  // Right-click → id-menu
  await rightClick($('.resid') as HTMLElement)
  await found('[data-testid="id-menu"]')
  expect(testid('id-list')).toBeNull()
})

test('right-click on ID badge opens the ID menu', async () => {
  serve()
  await shown('本文1')

  await rightClick(await found('.resid'))
  await found('[data-testid="id-menu"]')

  expect(screen.getByRole('button', { name: 'NGIDに追加' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'コピー' })).toBeTruthy()
  expect(screen.getByRole('button', { name: '取得済みスレから検索' })).toBeTruthy()
})

test('NG post: reason header is struck-through and click toggles the body', async () => {
  serve({ ngIds: [scopedNgId('target')] })

  // Both "target" reses are hidden; res 2 is visible.
  await shown('本文2')
  expect(texts('本文1')).toHaveLength(0)
  expect(texts('本文3')).toHaveLength(0)

  // The reason header is a <del class="ng">: struck through.
  expect($$('del.ng')).toHaveLength(2) // two posts with ID:target
  const res1Ng = ngHeader('1 NG ID')
  expect(res1Ng.textContent?.trim()).toBe('1 NG ID')

  // Clicking the struck-through header reveals that post's body only.
  await click(res1Ng)
  await shown('本文1')
  expect(texts('本文3')).toHaveLength(0)
  const res1 = $('.res[data-res="1"]') as HTMLElement
  expect(within(res1).queryAllByText('ID:target')).toHaveLength(0)

  // Clicking again hides it.
  await click(res1Ng)
  await until(() => expect(texts('本文1')).toHaveLength(0))
})

test('NGID追加 adds ID to NG and the ID menu does not expose removal', async () => {
  const added: unknown[] = []
  const api = serve({ ngIds: () => (added.length > 0 ? [scopedNgId('target')] : []) })
  api.on(
    /\/api\/ng-ids(\?|$)/,
    ({ body }: { body: unknown }) => {
      added.push(body)
      return { ok: true }
    },
    'POST',
  )
  await shown('本文1')

  await rightClick(await found('.resid'))
  await found('[data-testid="id-menu"]')

  await click(screen.getByRole('button', { name: 'NGIDに追加' }))

  await until(() => expect(added).toHaveLength(1))
  expect(added[0]).toEqual({ server: 'egg', board: 'applism', ng_id: 'target' })

  // After adding, the posts show their struck-through NG reason.
  await found('del.ng')
  // The NG post's ID is hidden, and the ID menu offers no NG ID removal.
  expect($('.resid[data-id="target"]')).toBeNull()
  expect(screen.queryAllByRole('button', { name: 'NGIDから削除' })).toHaveLength(0)
})

test('NG post right-click opens its reason menu and removes the NG ID', async () => {
  const api = serve({ ngIds: [scopedNgId('target')] })
  await until(() => expect(ngHeader('1 NG ID')?.textContent?.trim()).toBe('1 NG ID'))

  await rightClick(ngHeader('1 NG ID'))
  await found('[data-testid="ng-menu"]')
  expect(screen.getByRole('button', { name: 'NG IDから削除' })).toBeTruthy()
  expect(testid('reply-menu')).toBeNull()

  await click(screen.getByRole('button', { name: 'NG IDから削除' }))
  await until(() => expect(api.calls('/api/ng-ids', 'DELETE')).toHaveLength(1))
  const [removed] = api.calls('/api/ng-ids', 'DELETE')
  expect(Object.fromEntries(new URLSearchParams(removed.search))).toEqual({
    server: 'egg',
    board: 'applism',
    ng_id: 'target',
  })
})

test('NG post long-press opens only its reason menu without toggling the body', async () => {
  serve({ ngIds: [scopedNgId('target')] })

  const header = await found('del.ng')
  header.dispatchEvent(
    new PointerEvent('pointerdown', { bubbles: true, pointerType: 'touch', pointerId: 1 }),
  )
  await new Promise((done) => setTimeout(done, 550))
  header.dispatchEvent(
    new PointerEvent('pointerup', { bubbles: true, pointerType: 'touch', pointerId: 1 }),
  )
  header.dispatchEvent(new MouseEvent('click', { bubbles: true }))

  await found('[data-testid="ng-menu"]')
  expect(testid('reply-menu')).toBeNull()
  expect(texts('本文1')).toHaveLength(0)
})

test('copy writes "ID:xxx" to clipboard', async () => {
  serve()
  await shown('本文1')
  await rightClick(await found('.resid'))
  await click(screen.getByRole('button', { name: 'コピー' }))

  await until(() => expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('ID:target'))
})

test('id-search shows results modal', async () => {
  serve({
    searchResult: [
      {
        thread_id: '1771127145',
        title: 'NGテストスレ',
        res: [res(1, 'target', '検索ヒット本文', '2025/01/01')],
      },
    ],
  })
  await shown('本文1')
  await rightClick(await found('.resid'))
  await click(screen.getByRole('button', { name: '取得済みスレから検索' }))

  await found('[data-testid="id-search-result"]')
  await shown('検索ヒット本文')
})

test('id-search shows 該当なし when no results', async () => {
  serve({ searchResult: [] })
  await shown('本文1')
  await rightClick(await found('.resid'))
  await click(screen.getByRole('button', { name: '取得済みスレから検索' }))

  await found('[data-testid="id-search-result"]')
  await shown('該当なし')
})

test('ID menu closes via × button', async () => {
  serve()
  await shown('本文1')
  await rightClick(await found('.resid'))
  await found('[data-testid="id-menu"]')

  await click(screen.getByRole('button', { name: '閉じる' }))
  await until(() => expect(testid('id-menu')).toBeNull())
})

test('NG body is hidden inside the anchor tree (anchorNode uses resHeadAndBody)', async () => {
  serve({
    ngIds: [scopedNgId('target')],
    dat: {
      title: FAV.title,
      res_count: 2,
      read_res: 0,
      status: 'active',
      res: [
        res(1, 'normal', 'アンカー &gt;&gt;2'),
        res(2, 'target', 'NG本文がここに表示されてはいけない'),
      ],
    },
  })

  await click(await found('.anchor[data-anchor="2"]'))
  const modal = await found('.modal')

  // The NG body must NOT be in the modal; the NG reason header is shown instead.
  expect(within(modal).queryAllByText(/NG本文がここに表示されてはいけない/)).toHaveLength(0)
  expect($$('del.ng', modal)).toHaveLength(1)
})

test('single-occurrence ID shows a clickable resid span', async () => {
  serve({
    dat: {
      title: FAV.title,
      res_count: 2,
      read_res: 0,
      status: 'active',
      res: [
        res(1, 'uniq1', '本文A', '2025/01/01 00:00:00'),
        res(2, 'uniq2', '本文B', '2025/01/01 00:01:00'),
      ],
    },
  })
  await shown('本文A')

  // Both reses have unique IDs → each still shows a clickable .resid span.
  expect($$('.resid')).toHaveLength(2)

  await rightClick($('.resid') as HTMLElement)
  await found('[data-testid="id-menu"]')
  expect(screen.getByRole('button', { name: 'NGIDに追加' })).toBeTruthy()
})
