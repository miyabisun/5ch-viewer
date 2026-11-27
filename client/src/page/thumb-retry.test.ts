import { expect, test, vi } from 'vitest'

import { $, click, found, open } from '../testing/page'
import { server } from '../testing/server'

const fav = {
  server: 'egg',
  board: 'applism',
  board_name: 'アプリ',
  thread_id: '123',
  title: '画像の後追い取得',
  res_count: 1,
  read_res: 0,
  rating: 0,
  status: 'active',
}

function serve() {
  return server()
    .on('/api/favorites', [fav])
    .on(/\/dat$/, {
      ...fav,
      mosaic_urls: [],
      res: [
        {
          num: 1,
          name: '名無し',
          mail: '',
          date: '2026/10/06',
          id: null,
          body: '画像<br>https://example.com/late.png',
        },
      ],
    })
    .on(/\/api\/(ng-ids|ng-words|ng-wacchoi)$/, [])
}

/**
 * Opens the thread and watches its thumbnail: each time the page sets its `src` is a request
 * for the image (the browser fetches it again even when it is the same), which the test then
 * answers with the `error` (the server's 404) or the `load` the browser would fire.
 */
async function openThumb() {
  open(serve(), '/')
  await click(await found('.info'))
  const img = (await found('.thread-body .thumb')) as HTMLImageElement
  const own = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src')!
  const requests = { count: 1 }
  Object.defineProperty(img, 'src', {
    configurable: true,
    get: () => own.get!.call(img),
    set: (value) => {
      requests.count += 1
      own.set!.call(img, value)
    },
  })
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  const answer = (kind: 'error' | 'load') => img.dispatchEvent(new Event(kind))
  return { img, requests, answer }
}

const crossShown = (img: HTMLImageElement) => img.classList.contains('thumb-missing')

test('a thumbnail requested before its prefetch finishes loads once the image is cached', async () => {
  const { img, requests, answer } = await openThumb()
  // The first request 404s (the image is not cached yet): no cross, a retry is scheduled.
  answer('error')
  expect(requests.count).toBe(1)
  expect(crossShown(img)).toBe(false)

  await vi.advanceTimersByTimeAsync(30_000)
  expect(requests.count).toBe(2)
  // The image is cached by now: the retry loads it.
  answer('load')
  expect(crossShown(img)).toBe(false)
  expect($('.thread-body .thumb-missing')).toBeNull()
})

test('a thumbnail that never becomes available shows the cross after a bounded number of retries', async () => {
  const { img, requests, answer } = await openThumb()
  answer('error')
  expect(requests.count).toBe(1)

  // Every request 404s: the page retries a few times, then shows the cross.
  for (let round = 0; round < 20 && !crossShown(img); round++) {
    const before = requests.count
    await vi.advanceTimersByTimeAsync(5_000)
    if (requests.count > before) answer('error')
  }
  expect(crossShown(img)).toBe(true)
  const asked = requests.count
  expect(asked).toBeGreaterThan(1)

  // No further requests once it has given up.
  await vi.advanceTimersByTimeAsync(600_000)
  expect(requests.count).toBe(asked)
})
