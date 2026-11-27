import { screen, within } from '@testing-library/svelte'
import { describe, expect, test } from 'vitest'

import {
  $,
  $$,
  back,
  click,
  fill,
  found,
  open,
  reload,
  seen,
  settle,
  swipe,
  until,
} from '../testing/page'
import { server, type Asked } from '../testing/server'

const THREAD = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '1771127145',
  title: '未読スレ',
  rating: 0,
  status: 'active',
}
const OTHER = { ...THREAD, thread_id: '1771127146', title: '別のスレ' }

function post(num: number, own = false) {
  return {
    num,
    name: '名無し',
    mail: '',
    date: `2025/01/01 00:00 ID:x${num}`,
    body: Array.from({ length: (num % 3) + 1 }, () => `本文${num}`).join('<br>'),
    id: `x${num}`,
    own,
  }
}

interface Thread {
  meta: typeof THREAD
  res: ReturnType<typeof post>[]
  read_res: number
  arrivals?: ReturnType<typeof post>[]
}

function threadState(meta: typeof THREAD, count: number, readRes: number): Thread {
  return { meta, res: Array.from({ length: count }, (_, i) => post(i + 1)), read_res: readRes }
}

// A small stateful server: reload appends what was posted, progress raises read_res.
function serve(threads: Thread[]) {
  const progress: { thread: string; read_res: number }[] = []
  const byId = new Map(threads.map((t) => [t.meta.thread_id, t]))
  const find = ({ path }: Asked) =>
    byId.get(path.match(/\/api\/favorites\/[^/]+\/[^/]+\/([^/]+)\//)![1])!
  const row = (t: Thread) => ({
    ...t.meta,
    res_count: t.res.length,
    read_res: t.read_res,
    own_res: t.res.filter((r) => r.own).map((r) => r.num),
  })
  const api = server()
    .on('/api/favorites', () => threads.map(row))
    .on(/\/api\/favorites\/.+\/dat$/, (asked: Asked) => {
      const t = find(asked)
      return {
        title: t.meta.title,
        res_count: t.res.length,
        read_res: t.read_res,
        status: 'active',
        mosaic_urls: [],
        res: t.res,
      }
    })
    .on(/\/api\/favorites\/.+\/reload$/, (asked: Asked) => {
      const t = find(asked)
      t.res.push(...(t.arrivals ?? []))
      t.arrivals = []
      return { res_count: t.res.length, read_res: t.read_res, status: 'active' }
    })
    .on(/\/api\/favorites\/.+\/post$/, (asked: Asked) => {
      const t = find(asked)
      t.arrivals = [...(t.arrivals ?? []), post(t.res.length + (t.arrivals?.length ?? 0) + 1, true)]
      return { ok: true }
    })
    .on(/\/api\/favorites\/.+\/progress$/, (asked: Asked) => {
      const t = find(asked)
      if (asked.method === 'POST') {
        const n = asked.body.read_res
        progress.push({ thread: t.meta.thread_id, read_res: n })
        t.read_res = Math.max(t.read_res, n)
      }
      return { ok: true }
    })
    .on('/api/ng-ids', [])
    .on(/\/api\/ng-words(\?|$)/, [])
    .on('/api/ng-wacchoi', [])
  return { api, progress }
}

const row = (title: string) =>
  $$('.list-pane .thread').find((el) => el.textContent?.includes(title)) as HTMLElement | undefined
const badge = (title: string) => (row(title) ? $('.unread', row(title)) : null)

/**
 * Opens a thread from the list and waits until it is laid out, and shows its newest posts:
 * the browser tells the page what is in sight on the screen it opens on.
 */
async function openFromList(title: string, inSight: number[]) {
  await click((await until(() => row(title) ?? Promise.reject(new Error(title)))) as HTMLElement)
  await found('[data-testid="read-boundary"]')
  // once the page tracks what is read, the list's badge follows what was seen
  await until(() => {
    seen(...inSight.map((num) => $(`.thread-body > .res[data-res="${num}"]`) as HTMLElement))
    expect(badge(title)).toBeNull()
  })
  await settle()
}

const leaveBy: Record<string, () => Promise<unknown>> = {
  'the favorites tab': () => click(screen.getByTestId('tab-favorites')),
  'the browser back button': () => back(),
  'a back swipe': async () => swipe($('.thread-body') as HTMLElement, 140, 0, 20, 300),
}

describe('phone: leaving right after a single new post was shown', () => {
  for (const [how, leave] of Object.entries(leaveBy)) {
    test(`saves the read post when leaving by ${how}`, async () => {
      const thread = threadState(THREAD, 31, 30)
      const { api, progress } = serve([thread])
      open(api, '/')
      await until(() => expect(badge(THREAD.title)?.textContent).toBe('1'))

      // Post 31 is on the screen the thread opens on.
      await openFromList(THREAD.title, [31])
      await leave()
      await until(() => expect(row(THREAD.title)).toBeTruthy())
      await until(() => expect(thread.read_res).toBe(31))

      reload(api)
      await until(() => expect(row(THREAD.title)).toBeTruthy())
      expect(badge(THREAD.title)).toBeNull()
      expect(progress.at(-1)).toEqual({ thread: THREAD.thread_id, read_res: 31 })
    })
  }
})

describe('PC: switching threads in the two-column layout', () => {
  test('saves the previous thread’s read position', async () => {
    const first = threadState(THREAD, 31, 30)
    const second = threadState(OTHER, 10, 10)
    const { api } = serve([first, second])
    open(api, '/')

    await openFromList(THREAD.title, [31])
    await click(row(OTHER.title) as HTMLElement)
    await until(() => expect(screen.getByTestId('thread-title').textContent).toBe(OTHER.title))
    await until(() => expect(first.read_res).toBe(31))
  })
})

describe('list: own posts are not unread', () => {
  test('the badge counts only other people’s posts after the read position', async () => {
    const ownOnly = threadState(THREAD, 60, 60)
    ownOnly.res.push(post(61, true))
    const mixed = threadState(OTHER, 60, 50)
    mixed.res.push(post(61, true), post(62))
    const { api } = serve([ownOnly, mixed])
    open(api, '/')

    await until(() => expect(row(THREAD.title)).toBeTruthy())
    expect(badge(THREAD.title)).toBeNull()
    // 51..62 is 12 posts, one of them the user's own.
    await until(() => expect(badge(OTHER.title)?.textContent).toBe('11'))
  })
})

describe('phone: returning to the list after posting', () => {
  for (const [how, leave] of Object.entries(leaveBy)) {
    test(`shows the new post count when returning by ${how}`, async () => {
      const thread = threadState(THREAD, 20, 20)
      const { api } = serve([thread])
      open(api, '/')
      await openFromList(THREAD.title, [20])
      await click(screen.getByRole('button', { name: '書き込む' }))
      await fill(await found('.post-textarea'), 'テスト')
      await click($('.post-submit') as HTMLElement)
      await until(() => expect($$('.thread-body > .res.own')).toHaveLength(1))

      await leave()
      await until(() => expect(row(THREAD.title)?.textContent).toContain('アプリ 21'))
      expect(badge(THREAD.title)).toBeNull()
    })
  }
})
