import { screen } from '@testing-library/svelte'
import { expect, test } from 'vitest'

import { $, $$, click, found, open, rightClick, shown, until } from '../testing/page'
import { server } from '../testing/server'

const FAVS = [0, 1, 2, 3, 4, 5].map((r) => ({
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: `100000000${r}`,
  title: `スレ${r}`,
  res_count: 10,
  read_res: 10,
  rating: r,
  status: 'active',
}))

/** Records the bodies of PATCH .../rating. */
function rating(api: ReturnType<typeof server>) {
  const sent: unknown[] = []
  api.on(/\/api\/favorites\/.+\/rating$/, ({ body }: { body: unknown }) => {
    sent.push(body)
    return { ok: true }
  })
  return sent
}

const menu = () => $('.menu')

test('right-click opens the action menu and rating change is sent', async () => {
  const api = server().on('/api/favorites', FAVS)
  const sent = rating(api)
  open(api, '/')

  // No menu before interaction.
  await found('.thread.rate-0')
  expect(menu()).toBeNull()

  await rightClick($('.thread.rate-0') as HTMLElement)
  await until(() => expect(menu()).not.toBeNull())

  // The read.cgi URL is shown as plain text.
  expect($('.menu-url')?.textContent).toBe('https://egg.5ch.io/test/read.cgi/applism/1000000000/')

  // Choose ★4 → PATCH rating {rating:4}, menu closes.
  await click($('.star[data-rating="4"]') as HTMLElement)
  await until(() => expect(sent).toHaveLength(1))
  expect(sent[0]).toEqual({ rating: 4 })

  await until(() => expect(menu()).toBeNull())
})

test('stars reflect the current rating with color, and modal closes via × / scrim', async () => {
  open(server().on('/api/favorites', FAVS), '/')

  await rightClick(await found('.thread.rate-3'))
  // rating 3: ★1..3 lit (on), ★4..5 unlit (off) + ☆ = 3 off.
  await until(() => expect($$('.star.on')).toHaveLength(3))
  expect($$('.star.off')).toHaveLength(3)
  // Unlit stars use the same ★ glyph (tinted by color), not ☆.
  for (const n of [3, 4, 5]) expect($(`.star[data-rating="${n}"]`)?.textContent).toBe('★')

  // × (top-right) closes the modal.
  await click(screen.getByRole('button', { name: '閉じる' }))
  await until(() => expect(menu()).toBeNull())

  // Outside (scrim) click also closes it.
  await rightClick($('.thread.rate-3') as HTMLElement)
  await until(() => expect(menu()).not.toBeNull())
  await click($('.modal-bg') as HTMLElement)
  await until(() => expect(menu()).toBeNull())

  // No bottom 閉じる action button anymore.
  await rightClick($('.thread.rate-3') as HTMLElement)
  await until(() => expect(menu()).not.toBeNull())
  expect($('.action.close')).toBeNull()
})

test('☆ (data-rating=0) clears rating to 0 unconditionally', async () => {
  const api = server().on('/api/favorites', FAVS)
  const sent = rating(api)
  open(api, '/')

  await rightClick(await found('.thread.rate-3'))
  await click(await found('.star[data-rating="0"]'))
  await until(() => expect(sent).toHaveLength(1))
  expect(sent[0]).toEqual({ rating: 0 })
})

test('copy actions write title / url / share text to the clipboard', async () => {
  open(server().on('/api/favorites', FAVS), '/')

  const url = 'https://egg.5ch.io/test/read.cgi/applism/1000000003/'
  const copied = () => (navigator.clipboard.writeText as any).mock.calls.at(-1)?.[0]

  await rightClick(await found('.thread.rate-3'))
  await click(screen.getByRole('button', { name: 'タイトルをコピー' }))
  await until(() => expect(copied()).toBe('スレ3'))

  await rightClick($('.thread.rate-3') as HTMLElement)
  await click(screen.getByRole('button', { name: 'URL をコピー' }))
  await until(() => expect(copied()).toBe(url))

  await rightClick($('.thread.rate-3') as HTMLElement)
  await click(screen.getByRole('button', { name: 'タイトル+URL をコピー' }))
  await until(() => expect(copied()).toBe(`スレ3\n${url}`))
})

test('plain click still opens the thread', async () => {
  const api = server()
    .on('/api/favorites', FAVS)
    .on(/\/api\/favorites\/.+\/dat$/, {
      title: 'スレ3',
      res_count: 1,
      read_res: 0,
      status: 'active',
      res: [{ num: 1, name: '名無し', mail: '', date: '2025 ID:x', body: '本文1' }],
    })
    .on(/\/api\/favorites\/.+\/reload$/, { res_count: 1, read_res: 0, status: 'active' })
  open(api, '/')

  await click(await found('.thread.rate-3 .info'))
  await shown('本文1')
})
