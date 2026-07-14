// Integration tests: image cache prefetch, SSRF guard, mosaic API.
//
// The itest-server mock provides:
//   GET /mock/img/:file              — serves a tiny PNG (or a large body for size tests)
//   /_control/image-hits/:file       — returns hit count for a file
//   /_control/image-size             — programs a large Content-Length for a file
//   /_control/image-content-type     — overrides Content-Type for a file (MIME rejection test)
//
// FIVECH_ALLOW_LOOPBACK_FOR_TEST is set in playwright.integration.config.js so that
// the Rust backend can download images from http://127.0.0.1:{MOCK_PORT}/mock/img/.
// Private IPs other than loopback (10.x, 172.16-31.x, 192.168.x, etc.) remain blocked.

import { test, expect, request as pwRequest } from '@playwright/test'
import { APP_URL, MOCK_URL } from '../playwright.integration.config.js'

const FAV = {
  server: 'egg',
  board: 'imgtest',
  thread_id: '9900000001',
  title: '画像キャッシュテストスレ',
}

// Build a mock image URL served by the itest mock server.
function mockImgUrl(file) {
  return `${MOCK_URL}/mock/img/${file}`
}

// The cache path that Rust's normalize_image_path produces for a loopback mock URL.
// MOCK_URL = http://127.0.0.1:3002 → path = "127.0.0.1:3002/mock/img/{file}"
function mockImgPath(file) {
  return `127.0.0.1:3002/mock/img/${file}`
}

async function reset(ctx) {
  expect((await ctx.post(`${APP_URL}/_control/reset`)).ok()).toBeTruthy()
  expect((await ctx.post(`${MOCK_URL}/_control/reset`)).ok()).toBeTruthy()
}

async function seedFavorite(ctx, opts = {}) {
  const r = await ctx.post(`${APP_URL}/_control/seed-favorite`, {
    data: { ...FAV, res_count: 5, blob_posts: 5, ...opts },
  })
  expect(r.ok()).toBeTruthy()
}

async function programThread(ctx, opts = {}) {
  const r = await ctx.post(`${MOCK_URL}/_control/thread`, {
    data: { ...FAV, res_count: 5, dat_posts: 5, ...opts },
  })
  expect(r.ok()).toBeTruthy()
}

async function reloadThread(ctx) {
  const r = await ctx.get(
    `${APP_URL}/api/favorites/${FAV.server}/${FAV.board}/${FAV.thread_id}/reload`,
  )
  expect(r.ok()).toBeTruthy()
  return r.json()
}

async function getDat(ctx) {
  const r = await ctx.get(
    `${APP_URL}/api/favorites/${FAV.server}/${FAV.board}/${FAV.thread_id}/dat`,
  )
  expect(r.ok()).toBeTruthy()
  return r.json()
}

async function imageHits(ctx, file) {
  const r = await ctx.get(`${MOCK_URL}/_control/image-hits/${file}`)
  expect(r.ok()).toBeTruthy()
  return r.json()
}

async function seedImage(ctx, { url, path, mime = 'image/png' }) {
  // Directly seed image_cache via the app control endpoint.
  const r = await ctx.post(`${APP_URL}/_control/seed-image`, {
    data: { url, path, mime, mosaic: 0 },
  })
  return r.ok()
}

// Poll until fn() returns true or timeout (ms) is reached.
async function poll(fn, { timeout = 3000, interval = 100 } = {}) {
  const deadline = Date.now() + timeout
  while (Date.now() < deadline) {
    if (await fn()) return true
    await new Promise((r) => setTimeout(r, interval))
  }
  return false
}

test.beforeEach(async () => {
  const ctx = await pwRequest.newContext()
  await reset(ctx)
  await ctx.dispose()
})

// ---------------------------------------------------------------------------
// Serve-endpoint smoke test (seed-image control path)
// ---------------------------------------------------------------------------

// Case 1: seeded image is served via GET /api/images/{path}.
// Tests the serve endpoint directly without going through the HTTP prefetch pipeline.
test('seeded image is served via GET /api/images/{path}', async ({ request }) => {
  const imageUrl = 'https://example.com/test-image.png'
  const imagePath = 'example.com/test-image.png'

  // Seed image_cache directly (bypasses prefetch, tests the serve endpoint).
  const seeded = await seedImage(request, { url: imageUrl, path: imagePath })
  expect(seeded).toBe(true)

  // The image should now be available via the serve endpoint.
  const r = await request.get(`${APP_URL}/api/images/${imagePath}`)
  expect(r.ok()).toBeTruthy()
  expect(r.headers()['content-type']).toContain('image/png')
  expect(r.headers()['x-content-type-options']).toBe('nosniff')
  expect(r.headers()['cache-control']).toContain('max-age=31536000')
})

// ---------------------------------------------------------------------------
// Real HTTP prefetch pipeline tests (cases a–d)
// ---------------------------------------------------------------------------

// Case a: dat with image URL → after reload, image_cache has BLOB (real prefetch path).
// The dat body contains a loopback mock URL; FIVECH_ALLOW_LOOPBACK_FOR_TEST permits download.
test('dat image URL triggers prefetch: image is cached after reload', async ({ request }) => {
  const file = 'test1.png'
  const imgUrl = mockImgUrl(file)

  await seedFavorite(request)
  // Program the mock thread with 10 posts; the dat grows from 5→10, triggering reload update.
  await programThread(request, { res_count: 10, dat_posts: 10, image_urls: [imgUrl] })

  const result = await reloadThread(request)
  expect(result.updated).toBe(true)

  // Poll until the image appears in the cache (prefetch runs in background).
  const cachePath = mockImgPath(file)
  const cached = await poll(async () => {
    const r = await request.get(`${APP_URL}/api/images/${cachePath}`)
    return r.status() === 200
  })
  expect(cached).toBe(true)

  // Confirm the mock was actually hit (real HTTP download occurred).
  const hits = await imageHits(request, file)
  expect(hits).toBe(1)
})

// Case b: second reload does not re-download an already-cached image.
test('cached image is not re-downloaded on second reload', async ({ request }) => {
  const file = 'test2.png'
  const imgUrl = mockImgUrl(file)

  await seedFavorite(request)
  await programThread(request, { res_count: 10, dat_posts: 10, image_urls: [imgUrl] })

  // First reload: triggers prefetch.
  await reloadThread(request)

  // Wait until cached.
  const cachePath = mockImgPath(file)
  await poll(async () => {
    const r = await request.get(`${APP_URL}/api/images/${cachePath}`)
    return r.status() === 200
  })
  const hitsAfterFirst = await imageHits(request, file)
  expect(hitsAfterFirst).toBe(1)

  // Second reload with more posts so subject.txt changes (mock res_count grows again).
  await programThread(request, { res_count: 15, dat_posts: 15, image_urls: [imgUrl] })
  await reloadThread(request)

  // Wait a tick for any background work.
  await new Promise((r) => setTimeout(r, 800))

  // Hit count must remain exactly 1: already cached, so no re-download.
  const hitsAfterSecond = await imageHits(request, file)
  expect(hitsAfterSecond).toBe(1)
})

// Case c: image larger than 5 MB is not cached.
test('image exceeding 5 MB is rejected and not cached', async ({ request }) => {
  const file = 'huge.png'
  const imgUrl = mockImgUrl(file)

  // Program the mock to serve a 6 MB body for this file.
  const sixMb = 6 * 1024 * 1024
  const sizeResp = await request.post(`${MOCK_URL}/_control/image-size`, {
    data: { file, size: sixMb },
  })
  expect(sizeResp.ok()).toBeTruthy()

  await seedFavorite(request)
  await programThread(request, { res_count: 10, dat_posts: 10, image_urls: [imgUrl] })
  await reloadThread(request)

  // Allow time for any (erroneous) prefetch to complete.
  await new Promise((r) => setTimeout(r, 1000))

  // The image must NOT be in the cache.
  const cachePath = mockImgPath(file)
  const r = await request.get(`${APP_URL}/api/images/${cachePath}`)
  expect(r.status()).toBe(404)
})

// Case d: image with non-image MIME type (text/html) is rejected and not cached.
test('image with text/html MIME type is rejected and not cached', async ({ request }) => {
  const file = 'html-page.png'
  const imgUrl = mockImgUrl(file)

  // Override the mock Content-Type to text/html for this file.
  const ctResp = await request.post(`${MOCK_URL}/_control/image-content-type`, {
    data: { file, content_type: 'text/html; charset=utf-8' },
  })
  expect(ctResp.ok()).toBeTruthy()

  await seedFavorite(request)
  await programThread(request, { res_count: 10, dat_posts: 10, image_urls: [imgUrl] })
  await reloadThread(request)

  // Allow time for any (erroneous) prefetch to complete.
  await new Promise((r) => setTimeout(r, 1000))

  // The image must NOT be in the cache (MIME rejected).
  const cachePath = mockImgPath(file)
  const r = await request.get(`${APP_URL}/api/images/${cachePath}`)
  expect(r.status()).toBe(404)
})

// ---------------------------------------------------------------------------
// SSRF guard tests (case e)
// ---------------------------------------------------------------------------

// Case e: SSRF guard — private IP (10.x) is blocked even with FIVECH_ALLOW_LOOPBACK_FOR_TEST.
// We use 10.0.0.1 (a non-loopback private IP) which is never allowed regardless of the env var.
test('SSRF: private IP (10.0.0.1) image is not downloaded', async ({ request }) => {
  const privateImg = 'http://10.0.0.1/x.png'

  await seedFavorite(request)
  await programThread(request, {
    res_count: 10,
    dat_posts: 10,
    image_urls: [privateImg],
  })

  const result = await reloadThread(request)
  expect(result.updated).toBe(true)

  // Allow time for any background prefetch attempt.
  await new Promise((r) => setTimeout(r, 500))

  // The image must NOT be in the cache (SSRF blocked).
  const cachePath = '10.0.0.1/x.png'
  const r = await request.get(`${APP_URL}/api/images/${cachePath}`)
  expect(r.status()).toBe(404)
})

// ---------------------------------------------------------------------------
// Mosaic API tests
// ---------------------------------------------------------------------------

// mosaic toggle: POST sets it, DELETE clears it, reflected in getDat mosaic_urls.
test('mosaic toggle: POST sets it, DELETE clears it, reflected in getDat mosaic_urls', async ({
  request,
}) => {
  const imageUrl = 'https://example.com/mosaic-test.png'
  const imagePath = 'example.com/mosaic-test.png'

  // Seed a favorite and an image in the cache.
  await seedFavorite(request)
  await seedImage(request, { url: imageUrl, path: imagePath })

  // Program the thread dat to contain the image URL so getDat sees it.
  await programThread(request, {
    res_count: 10,
    dat_posts: 5,
    image_urls: [imageUrl],
  })
  await reloadThread(request) // fetch the dat (grows from 5 to 10)

  // Initially no mosaic.
  let dat = await getDat(request)
  expect(dat.mosaic_urls).not.toContain(imageUrl)

  // Set mosaic.
  const setResp = await request.post(`${APP_URL}/api/images/mosaic`, {
    data: { url: imageUrl },
  })
  expect(setResp.ok()).toBeTruthy()
  const setBody = await setResp.json()
  expect(setBody.ok).toBe(true)

  // getDat must now include imageUrl in mosaic_urls.
  dat = await getDat(request)
  expect(dat.mosaic_urls).toContain(imageUrl)

  // Unset mosaic.
  const unsetResp = await request.delete(`${APP_URL}/api/images/mosaic`, {
    data: { url: imageUrl },
  })
  expect(unsetResp.ok()).toBeTruthy()

  // getDat must no longer include imageUrl in mosaic_urls.
  dat = await getDat(request)
  expect(dat.mosaic_urls).not.toContain(imageUrl)
})

// mosaic flag and image BLOB are independent — BLOB can be fetched even when mosaic=1.
test('image BLOB is accessible via serve endpoint regardless of mosaic flag', async ({
  request,
}) => {
  const imageUrl = 'https://example.com/blob-mosaic.png'
  const imagePath = 'example.com/blob-mosaic.png'

  await seedImage(request, { url: imageUrl, path: imagePath })

  // Set mosaic.
  await request.post(`${APP_URL}/api/images/mosaic`, { data: { url: imageUrl } })

  // The BLOB must still be served (mosaic is a display hint, not an access restriction).
  const imgResp = await request.get(`${APP_URL}/api/images/${imagePath}`)
  expect(imgResp.ok()).toBeTruthy()
  const ct = imgResp.headers()['content-type']
  expect(ct).toContain('image/png')
})

// Non-cached path returns 404.
test('uncached image path returns 404', async ({ request }) => {
  const r = await request.get(`${APP_URL}/api/images/nonexistent.com/no-such.png`)
  expect(r.status()).toBe(404)
})

// Mosaic URL validation: invalid URL returns 400.
test('mosaic POST rejects invalid URL', async ({ request }) => {
  const r = await request.post(`${APP_URL}/api/images/mosaic`, {
    data: { url: 'ftp://bad.com/x.png' },
  })
  expect(r.status()).toBe(400)
})
