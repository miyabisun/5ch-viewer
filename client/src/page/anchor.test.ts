import { screen, within } from '@testing-library/svelte'
import { describe, expect, test } from 'vitest'

import { $, $$, click, found, open, path, shown, swipe, texts, until } from '../testing/page'
import { server } from '../testing/server'

const FAV = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: 'テストスレ',
  res_count: 2,
  read_res: 0,
  rating: 0,
  status: 'active',
}

const THREAD_PATH = `/${FAV.server}/${FAV.board}/${FAV.thread_id}`

const res = (num: number, id: string, body: string) => ({
  num,
  name: '名無し',
  mail: '',
  date: `2025 ID:${id}`,
  body,
})

// Res 1 anchors to res 2 (>>2); res 2 anchors back to res 1 (>>1) — mutual cycle.
// The body is server-sanitized, so >> is &gt;&gt;.
function datResponse() {
  return {
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [res(1, 'a', '最初のレス &gt;&gt;2'), res(2, 'b', '&gt;&gt;1 これはアンカー')],
  }
}

function serve(dat: unknown = datResponse()) {
  const api = server()
    .on('/api/favorites', [FAV])
    .on(/\/api\/favorites\/.+\/dat$/, dat)
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 2, read_res: 0, status: 'active' })
  open(api, THREAD_PATH)
  return api
}

const modal = () => $('.modal') as HTMLElement
/** The reses in the modal whose number is `num`. */
const inModal = (num: number, selector = '.res .num') =>
  $$(selector, modal()).filter((el) => el.textContent?.trim() === String(num))

// Real 5ch dat bodies wrap >>N in <a href="../test/read.cgi/..."> tags; the server
// sanitizer (src/sanitize.rs) strips those <a> tags and leaves the plain &gt;&gt;N text.
// Given an already-sanitized body, the >>N text is linkified into a clickable .anchor span
// (not left as plain text or a raw link), and clicking it opens the modal.
test('a sanitized >>N in a dat body renders as a clickable .anchor span (not a raw link)', async () => {
  serve({
    title: FAV.title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [res(1, 'a', '最初のレス'), res(2, 'b', '&gt;&gt;1 read.cgiアンカーから変換')],
  })
  await shown(/read\.cgiアンカーから変換/)

  // The >>1 must be an .anchor span (not a raw <a> link) so clicking opens the modal.
  const anchor = await found('.anchor[data-anchor="1"]')
  expect(anchor.tagName).toBe('SPAN')

  await click(anchor)
  await until(() => expect(modal()).not.toBeNull())
  expect(within(modal()).getByText('最初のレス')).toBeTruthy()
})

test('clicking a >>N anchor opens the modal without navigating', async () => {
  serve()
  await shown(/これはアンカー/)

  // The anchor is a span, not a link (no navigation element).
  const anchor = await found('.anchor[data-anchor="1"]')
  expect(anchor.tagName).toBe('SPAN')

  await click(anchor)

  // Modal shows the referenced res; URL stays on the thread path; the ThreadView
  // (sticky title) is still mounted (we did not navigate to the list).
  await until(() => expect(modal()).not.toBeNull())
  expect(within(modal()).getByText(/最初のレス/)).toBeTruthy()
  expect(path()).toBe(THREAD_PATH)
  expect(screen.getByTestId('thread-title')).toBeTruthy()
})

// Single unified tree: ancestors (shallowest first) -> self -> descendants.
// Clicking >>1: res1 references >>2 so res2 is an ancestor; res2 references >>1 (cycle) so
// res2 is NOT duplicated as a child. res3 references >>1 (backref) so res3 is a descendant.
test('anchor tree: unified single tree with ancestors above self and children below', async () => {
  serve({
    title: FAV.title,
    res_count: 3,
    read_res: 0,
    status: 'active',
    res: [
      res(1, 'a', '最初のレス &gt;&gt;2'),
      res(2, 'b', '&gt;&gt;1 これはアンカー'),
      res(3, 'c', '&gt;&gt;1 別の返信'),
    ],
  })
  await click(await found('.anchor[data-anchor="1"]'))
  await until(() => expect(modal()).not.toBeNull())

  // All three reses must be shown.
  const tree = within(modal())
  expect(tree.getByText(/最初のレス/)).toBeTruthy()
  expect(tree.getByText(/これはアンカー/)).toBeTruthy()
  expect(tree.getByText(/別の返信/)).toBeTruthy()

  // Each res must appear exactly once (res2 cycle-deduplicated as ancestor only).
  for (const num of [1, 2, 3]) expect(inModal(num)).toHaveLength(1)

  // self (res1) must be highlighted; ancestor (res2) and child (res3) must not.
  expect(inModal(1, '.res.anchor-self .num')).toHaveLength(1)
  expect(inModal(2, '.res.anchor-self .num')).toHaveLength(0)
  expect(inModal(3, '.res.anchor-self .num')).toHaveLength(0)
})

// Clicking >>1 (res1 anchors to res2, res2 anchors back to res1) must show both res1 and res2
// in the tree and must not loop infinitely.
test('anchor tree expands recursively and stops at cycles', async () => {
  serve()
  await shown(/これはアンカー/)

  await click(await found('.anchor[data-anchor="1"]'))
  await until(() => expect(modal()).not.toBeNull())

  const tree = within(modal())
  expect(tree.getByText(/最初のレス/)).toBeTruthy()
  expect(tree.getByText(/これはアンカー/)).toBeTruthy()

  // Each res must appear exactly once (cycle-safe: res2 shown as parent only).
  expect(inModal(1)).toHaveLength(1)
  expect(inModal(2)).toHaveLength(1)
})

// DAG: res1 -> res2 and res1 -> res3; both res2 and res3 -> res4. Clicking >>4 from res2:
// res4 has no forward anchors (no parents); res2 and res3 are its children, each once.
test('anchor tree expands DAG without duplicating shared nodes', async () => {
  serve({
    title: FAV.title,
    res_count: 4,
    read_res: 0,
    status: 'active',
    res: [
      res(1, 'a', 'root &gt;&gt;2 &gt;&gt;3'),
      res(2, 'b', '&gt;&gt;4 経路A'),
      res(3, 'c', '&gt;&gt;4 経路B'),
      res(4, 'd', '共有ノード'),
    ],
  })
  await click(await found('.anchor[data-anchor="4"]'))
  await until(() => expect(modal()).not.toBeNull())

  const tree = within(modal())
  expect(tree.getByText(/共有ノード/)).toBeTruthy()
  expect(tree.getByText(/経路A/)).toBeTruthy()
  expect(tree.getByText(/経路B/)).toBeTruthy()

  for (const num of [2, 3, 4]) expect(inModal(num)).toHaveLength(1)
})

// The anchor-tree modal follows the shared modal convention: a top-right × and an outside
// (scrim) click both close it; there is no bottom "閉じる" button.
test('anchor modal closes via × and via outside click (no bottom 閉じる)', async () => {
  serve()
  await click(await found('.anchor[data-anchor="1"]'))
  await until(() => expect(modal()).not.toBeNull())

  // No bottom 閉じる button: the only close affordance is the top-right ×.
  expect(within(modal()).queryAllByText('閉じる')).toHaveLength(0)
  // The close affordance is a quiet icon button with an SVG x (DESIGN.md), not a text glyph.
  expect($('.modal-close svg')).not.toBeNull()
  expect($('.modal-close')?.textContent?.trim()).toBe('')

  // Top-right × closes it.
  await click($('.modal-close') as HTMLElement)
  await until(() => expect(modal()).toBeNull())

  // Reopen, then close via an outside (scrim) click.
  await click(await found('.anchor[data-anchor="1"]'))
  await until(() => expect(modal()).not.toBeNull())
  await click($('.modal-bg') as HTMLElement)
  await until(() => expect(modal()).toBeNull())
})

// Regression (issue #6 redux): a touch on an anchor must not be treated as a right-swipe-back.
// A genuine right-swipe on the body (not on an anchor) still leaves the thread.
describe('touch: anchor tap vs swipe-back', () => {
  test('a swipe starting on an anchor does not navigate back', async () => {
    serve()
    await shown(/これはアンカー/)

    // A clear right-swipe whose touch STARTS on the anchor must be ignored as a swipe (it is
    // an anchor interaction), so we stay on the thread.
    swipe(await found('.anchor[data-anchor="1"]'), 150)
    await new Promise((done) => setTimeout(done, 50))
    expect(path()).toBe(THREAD_PATH)
    expect(screen.getByTestId('thread-title')).toBeTruthy()
  })

  test('a clear right-swipe on the body returns to the list', async () => {
    serve()
    await shown(/これはアンカー/)

    // Swipe on the body (not on an anchor) -> back to the list (URL '/').
    swipe(await found('.thread-body'), 150)
    await until(() => expect(path()).toBe('/'))
    expect(texts(/これはアンカー/)).toHaveLength(0)
  })
})
