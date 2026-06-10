const BASE = window.__BASE_PATH__ || ''

async function request(method, path, body, opts = {}) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
    ...opts,
  })
  if (!res.ok) {
    let msg = `${method} ${path}: ${res.status}`
    try {
      const j = await res.json()
      if (j.error) msg = j.error
    } catch {
      /* ignore */
    }
    throw new Error(msg)
  }
  return res.json()
}

export const api = {
  listFavorites: () => request('GET', '/api/favorites'),
  addFavorite: (body) => request('POST', '/api/favorites', body),
  removeFavorite: (s, b, t) => request('DELETE', `/api/favorites/${s}/${b}/${t}`),
  getDat: (s, b, t) => request('GET', `/api/favorites/${s}/${b}/${t}/dat`),
  // Viewer-side refresh: GET (cache-or-fetch). `no-store` avoids serving a stale
  // browser-cached result; the server may fetch the dat from 5ch behind the scenes.
  reload: (s, b, t) =>
    request('GET', `/api/favorites/${s}/${b}/${t}/reload`, undefined, {
      cache: 'no-store',
    }),
  // Read position: GET fetches the saved value, POST saves it (POST so the
  // unload path can reuse the same route via sendBeacon).
  getProgress: (s, b, t) => request('GET', `/api/favorites/${s}/${b}/${t}/progress`),
  setProgress: (s, b, t, readRes) =>
    request('POST', `/api/favorites/${s}/${b}/${t}/progress`, { read_res: readRes }),
  setRating: (s, b, t, rating) =>
    request('PATCH', `/api/favorites/${s}/${b}/${t}/rating`, { rating }),
  search: (q) => request('GET', `/api/search?q=${encodeURIComponent(q)}`),
}

// Reliably send read progress on page unload (via sendBeacon).
export function beaconProgress(s, b, t, readRes) {
  const url = BASE + `/api/favorites/${s}/${b}/${t}/progress`
  const blob = new Blob([JSON.stringify({ read_res: readRes })], {
    type: 'application/json',
  })
  navigator.sendBeacon(url, blob)
}
