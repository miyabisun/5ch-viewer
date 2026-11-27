import { screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import {
  $,
  $$,
  click,
  fill,
  found,
  open,
  rightClick,
  shown,
  texts,
  until,
  visit,
} from '../testing/page'
import { server, type Asked } from '../testing/server'

// Board-scoped NG Word (and the poster-ID rule it can register alongside).
const FAV_A = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '板Aのスレ',
  res_count: 2,
  read_res: 0,
  rating: 0,
  status: 'active',
}
const FAV_B = {
  ...FAV_A,
  board: 'other',
  board_name: '別板',
  thread_id: '1771127146',
  title: '板Bのスレ',
}

const PATH_A = `/${FAV_A.server}/${FAV_A.board}/${FAV_A.thread_id}`
const PATH_B = `/${FAV_B.server}/${FAV_B.board}/${FAV_B.thread_id}`

const ARASHI = '荒らしの本文です'
const NORMAL = 'ふつうの本文'

function datResponse(title: string) {
  return {
    title,
    res_count: 2,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00 ID:arashi',
        body: ARASHI,
        id: 'arashi',
      },
      {
        num: 2,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:01:00.00 ID:normal',
        body: NORMAL,
        id: 'normal',
      },
    ],
  }
}

type Row = Record<string, unknown>

// Stateful mock of the board-scoped NG collections, so a rule saved from the modal is what the
// next GET returns (the page re-fetches after every change).
function setupRoutes({ ngIds = [] as Row[], ngWords = [] as Row[] } = {}) {
  const state = { ngIds: [...ngIds], ngWords: [...ngWords] }
  const collection =
    (name: 'ngIds' | 'ngWords', key: string[]) =>
    ({ method, body, search }: Asked) => {
      const rows = state[name]
      if (method === 'GET') return rows
      if (method === 'POST') {
        const dup = rows.some((r) => key.every((k) => r[k] === body[k]))
        if (!dup) rows.push({ ...body, created_at: 0 })
        return { ok: true }
      }
      const q = Object.fromEntries(new URLSearchParams(search))
      const i = rows.findIndex((r) => key.every((k) => r[k] === q[k]))
      if (i >= 0) rows.splice(i, 1)
      return { ok: true }
    }
  const api = server()
    .on('/api/favorites', [FAV_A, FAV_B])
    .on(/\/api\/favorites\/egg\/applism\/.+\/dat$/, datResponse(FAV_A.title))
    .on(/\/api\/favorites\/egg\/other\/.+\/dat$/, datResponse(FAV_B.title))
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 2, read_res: 0, status: 'active' })
    .on('/api/ng-wacchoi', ({ method }: Asked) => (method === 'GET' ? [] : { ok: true }))
    .on(/\/api\/ng-ids(\?|$)/, collection('ngIds', ['server', 'board', 'ng_id']))
    .on(/\/api\/ng-words(\?|$)/, collection('ngWords', ['server', 'board', 'kind', 'pattern']))
  return { api, state }
}

async function openNgWordForm(bodyText: string | RegExp) {
  await rightClick(await shown(bodyText))
  await found('[data-testid="reply-menu"]')
  await click(screen.getByRole('button', { name: 'NG Word に追加' }))
  await found('[data-testid="ng-word-form"]')
}

const form = () => $('[data-testid="ng-word-form"]')
const patternField = () => screen.getByLabelText('NG Word') as HTMLTextAreaElement
const alsoIdBox = () => screen.getByLabelText('投稿者IDもNG') as HTMLInputElement
const add = () => click(screen.getByRole('button', { name: '追加' }))
const ngHeader = (text: string) => $$('del.ng').find((el) => el.textContent?.includes(text))

test('the reply menu opens the NG Word modal prefilled with the res body', async () => {
  const { api } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  // Defaults: literal match, the body as the pattern, the poster ID offered and on.
  expect(screen.getByRole('button', { name: '文字列' }).getAttribute('aria-pressed')).toBe('true')
  expect(screen.getByRole('button', { name: '正規表現' }).getAttribute('aria-pressed')).toBe(
    'false',
  )
  expect(patternField().value).toBe(ARASHI)
  expect(alsoIdBox().checked).toBe(true)
  expect(alsoIdBox().disabled).toBe(false)
})

test('a long-press on the res body also reaches the NG Word modal', async () => {
  const { api } = setupRoutes()
  open(api, PATH_A)
  const body = await shown(ARASHI)

  body.dispatchEvent(
    new PointerEvent('pointerdown', { bubbles: true, pointerType: 'touch', pointerId: 1 }),
  )
  await new Promise((done) => setTimeout(done, 550))
  body.dispatchEvent(
    new PointerEvent('pointerup', { bubbles: true, pointerType: 'touch', pointerId: 1 }),
  )

  await found('[data-testid="reply-menu"]')
  await click(screen.getByRole('button', { name: 'NG Word に追加' }))
  await found('[data-testid="ng-word-form"]')
})

test('キャンセル closes the modal without saving anything', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  await click(screen.getByRole('button', { name: 'キャンセル' }))
  await until(() => expect(form()).toBeNull())
  expect(state.ngWords).toEqual([])
  expect(state.ngIds).toEqual([])
})

test('a literal rule hides the post right away and also registers the poster ID', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  await fill(patternField(), '荒らし')
  await add()

  await until(() => expect(form()).toBeNull())
  await until(() => expect(texts(ARASHI)).toHaveLength(0))
  // NG ID wins the reason when both rules match the same post.
  await until(() => expect(ngHeader('1 NG ID')).toBeTruthy())
  expect(texts(NORMAL)).toHaveLength(1)

  expect(state.ngWords).toEqual([
    { server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 },
  ])
  expect(state.ngIds).toEqual([{ server: 'egg', board: 'applism', ng_id: 'arashi', created_at: 0 }])
})

test('unchecking 投稿者IDもNG saves the word without the ID', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  await fill(patternField(), '荒らし')
  await click(alsoIdBox())
  await add()

  await until(() => expect(texts(ARASHI)).toHaveLength(0))
  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
  expect(state.ngWords).toHaveLength(1)
  expect(state.ngIds).toEqual([])
})

test('a regex rule is stored with kind=regex and hides matching posts', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  await click(screen.getByRole('button', { name: '正規表現' }))
  expect(screen.getByRole('button', { name: '正規表現' }).getAttribute('aria-pressed')).toBe('true')
  await fill(patternField(), '^荒ら.+です$')
  await click(alsoIdBox())
  await add()

  await until(() => expect(texts(ARASHI)).toHaveLength(0))
  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
  expect(texts(NORMAL)).toHaveLength(1)
  expect(state.ngWords).toEqual([
    { server: 'egg', board: 'applism', kind: 'regex', pattern: '^荒ら.+です$', created_at: 0 },
  ])
  expect(state.ngIds).toEqual([])
})

test('an invalid regex is rejected with an error and nothing is saved', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  await click(screen.getByRole('button', { name: '正規表現' }))
  await fill(patternField(), '荒らし(')
  await add()

  await until(() =>
    expect(screen.getByRole('alert').textContent).toBe('正規表現として解釈できません'),
  )
  expect(form()).not.toBeNull()
  expect(state.ngWords).toEqual([])
  expect(state.ngIds).toEqual([])
  // Switching back to a literal match makes the same text acceptable.
  await click(screen.getByRole('button', { name: '文字列' }))
  await until(() => expect(screen.queryAllByRole('alert')).toHaveLength(0))
  await add()
  await until(() => expect(form()).toBeNull())
  expect(state.ngWords).toHaveLength(1)
})

test('an empty pattern is rejected with an error and nothing is saved', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  await fill(patternField(), '')
  await add()

  await until(() =>
    expect(screen.getByRole('alert').textContent).toBe('パターンを入力してください'),
  )
  expect(state.ngWords).toEqual([])
})

test('the ID checkbox is disabled and off for a post without a poster ID', async () => {
  const { api } = setupRoutes()
  api.on(/\/api\/favorites\/egg\/applism\/.+\/dat$/, {
    title: FAV_A.title,
    res_count: 1,
    read_res: 0,
    status: 'active',
    res: [{ num: 1, name: '名無し', mail: '', date: '2025/01/01(水) 00:00:00.00', body: ARASHI }],
  })
  open(api, PATH_A)
  await openNgWordForm(ARASHI)

  expect(alsoIdBox().disabled).toBe(true)
  expect(alsoIdBox().checked).toBe(false)
})

test('adding the same rule twice is idempotent', async () => {
  const { api, state } = setupRoutes()
  open(api, PATH_A)

  await openNgWordForm(NORMAL)
  await fill(patternField(), '荒らし')
  await click(alsoIdBox())
  await add()
  await until(() => expect(texts(ARASHI)).toHaveLength(0))

  await openNgWordForm(NORMAL)
  await fill(patternField(), '荒らし')
  await click(alsoIdBox())
  await add()
  await until(() => expect(form()).toBeNull())

  expect(state.ngWords).toHaveLength(1)
})

test('a rule registered on one board does not hide the same body on another board', async () => {
  const { api } = setupRoutes({
    ngWords: [{ server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 }],
  })

  open(api, PATH_A)
  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
  expect(texts(ARASHI)).toHaveLength(0)

  visit(api, PATH_B)
  await shown(ARASHI)
  expect($$('del.ng')).toHaveLength(0)
})

test('an NG ID registered on one board does not hide the same ID on another board', async () => {
  const { api } = setupRoutes({
    ngIds: [{ server: 'egg', board: 'applism', ng_id: 'arashi', created_at: 0 }],
  })

  open(api, PATH_A)
  await until(() => expect(ngHeader('1 NG ID')).toBeTruthy())

  visit(api, PATH_B)
  await shown(ARASHI)
  expect($$('del.ng')).toHaveLength(0)
})

test('a rule applies to every thread of its board, including one opened later', async () => {
  const SECOND = { ...FAV_A, thread_id: '1771127147', title: '板Aの別スレ' }
  const { api } = setupRoutes()
  api.on('/api/favorites', [FAV_A, SECOND, FAV_B])

  open(api, PATH_A)
  await openNgWordForm(ARASHI)
  await fill(patternField(), '荒らし')
  await click(alsoIdBox())
  await add()
  await until(() => expect(texts(ARASHI)).toHaveLength(0))

  visit(api, `/${FAV_A.server}/${FAV_A.board}/${SECOND.thread_id}`)
  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
  expect(texts(ARASHI)).toHaveLength(0)
})

test('the hidden post offers NG Word removal and the body comes back', async () => {
  const { api, state } = setupRoutes({
    ngWords: [{ server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 }],
  })
  open(api, PATH_A)

  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
  await rightClick(ngHeader('1 NG Word') as HTMLElement)
  await found('[data-testid="ng-menu"]')
  await click(screen.getByRole('button', { name: 'NG Wordから削除' }))

  await shown(ARASHI)
  await until(() => expect($$('del.ng')).toHaveLength(0))
  expect(state.ngWords).toEqual([])
})

test('NG ID takes precedence, and removing it reveals the remaining NG Word reason', async () => {
  const { api } = setupRoutes({
    ngIds: [{ server: 'egg', board: 'applism', ng_id: 'arashi', created_at: 0 }],
    ngWords: [{ server: 'egg', board: 'applism', kind: 'text', pattern: '荒らし', created_at: 0 }],
  })
  open(api, PATH_A)

  await until(() => expect(ngHeader('1 NG ID')).toBeTruthy())
  await rightClick(ngHeader('1 NG ID') as HTMLElement)
  await click(screen.getByRole('button', { name: 'NG IDから削除' }))

  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
  expect(texts(ARASHI)).toHaveLength(0)
})

test('a rule matches the display text, not the body markup', async () => {
  const { api } = setupRoutes()
  api.on(/\/api\/favorites\/egg\/applism\/.+\/dat$/, {
    title: FAV_A.title,
    res_count: 1,
    read_res: 0,
    status: 'active',
    res: [
      {
        num: 1,
        name: '名無し',
        mail: '',
        date: '2025/01/01(水) 00:00:00.00 ID:arashi',
        body: '1行目<br>a &amp; b',
        id: 'arashi',
      },
    ],
  })
  open(api, PATH_A)
  await openNgWordForm(/1行目/)

  // The prefill is the text a reader sees: <br> is a newline, &amp; is &.
  expect(patternField().value).toBe('1行目\na & b')

  await fill(patternField(), 'a & b')
  await click(alsoIdBox())
  await add()
  await until(() => expect(ngHeader('1 NG Word')).toBeTruthy())
})
