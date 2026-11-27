import { screen, within } from '@testing-library/svelte'
import { expect, test, vi } from 'vitest'

import { $, $$, click, found, open, seen, until } from '../testing/page'
import { server, type Asked } from '../testing/server'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '逆順テストスレ',
  res_count: 120,
  read_res: 100,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

function datResponse(fav = FAV) {
  return {
    title: fav.title,
    res_count: fav.res_count,
    read_res: fav.read_res,
    status: fav.status,
    mosaic_urls: [],
    res: Array.from({ length: fav.res_count }, (_, i) => ({
      num: i + 1,
      name: '名無し',
      mail: '',
      date: `2025/01/01 00:00 ID:x${i + 1}`,
      body: `本文${i + 1}`,
      id: `x${i + 1}`,
      own: false,
    })),
  }
}

function setup(fav = FAV) {
  return server()
    .on('/api/favorites', [fav])
    .on(/\/api\/favorites\/.+\/dat$/, datResponse(fav))
    .on(/\/api\/favorites\/.+\/progress$/, { ok: true })
    .on('/api/ng-ids', [])
    .on(/\/api\/ng-words(\?|$)/, [])
    .on('/api/ng-wacchoi', [])
}

const posts = () => $$('.thread-body > .res')
const after = (element: Element | null) => element?.nextElementSibling as HTMLElement | null

test('an entirely unread thread keeps one boundary after res 1', async () => {
  open(setup({ ...FAV, read_res: 0 }), THREAD_PATH)

  await until(() => expect(posts()).toHaveLength(120))
  expect(posts()[0].dataset.res).toBe('120')
  expect(posts().at(-1)?.dataset.res).toBe('1')
  expect(screen.getAllByTestId('read-boundary')).toHaveLength(1)
  expect(after(posts().at(-1) as HTMLElement)?.dataset.testid).toBe('read-boundary')
  expect(screen.getByTestId('thread-end').textContent?.trim()).toBe('おわり')
})

test('progress tracking starts when an initially empty thread gains a post', async () => {
  const emptyFav = { ...FAV, res_count: 0, read_res: 0 }
  let count = 0
  let savedProgress: number | null = null

  const api = setup(emptyFav)
    .on(/\/api\/favorites\/.+\/dat$/, () => datResponse({ ...emptyFav, res_count: count }))
    .on(/\/api\/favorites\/.+\/reload$/, () => {
      count = 1
      return { res_count: 1, read_res: 0, status: 'active' }
    })
    .on(/\/api\/favorites\/.+\/progress$/, ({ body }: Asked) => {
      savedProgress = body.read_res
      return { ok: true }
    })
  open(api, THREAD_PATH)
  await found('.thread-body')
  expect(posts()).toHaveLength(0)
  await new Promise((done) => requestAnimationFrame(done))

  await click(within($('.detail-pane') as HTMLElement).getByRole('button', { name: '更新' }))
  await until(() => expect(posts()).toHaveLength(1))
  // The new card is watched: once it is in sight, its position is saved.
  await until(() => {
    seen(posts()[0])
    expect(savedProgress).toBe(1)
  }, 4000)
}, 10_000)

test('switching threads cancels the previous view restoration frames', async () => {
  const secondFav = { ...FAV, thread_id: '1771127146', title: '切替先スレ' }
  const frames = new Map<number, FrameRequestCallback>()
  let nextFrameId = 1
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    const id = nextFrameId++
    frames.set(id, callback)
    return id
  })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id))
  const runAllFrames = () => {
    while (frames.size > 0) {
      const jobs = [...frames.values()]
      frames.clear()
      for (const callback of jobs) callback(performance.now())
    }
  }
  const api = setup()
    .on('/api/favorites', [FAV, secondFav])
    .on(/\/api\/favorites\/.+\/dat$/, ({ path }: Asked) =>
      datResponse(path.includes(secondFav.thread_id) ? secondFav : FAV),
    )
  open(api, '/')

  await click(await found('.info'))
  await found('[data-testid="read-boundary"]')
  await until(() => expect(frames.size).toBe(1))

  await click($$('.info')[1])
  await until(() => expect(screen.getByTestId('thread-title').textContent).toBe(secondFav.title))
  await found('[data-testid="read-boundary"]')
  await until(() => expect(frames.size).toBe(1))
  runAllFrames()
  expect(screen.getByTestId('thread-title').textContent).toBe(secondFav.title)
  expect(frames.size).toBe(0)
})
