const BASE = window.__BASE_PATH__ || ''

async function request(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : {},
    body: body ? JSON.stringify(body) : undefined,
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
  reload: (s, b, t) => request('POST', `/api/favorites/${s}/${b}/${t}/reload`),
  setProgress: (s, b, t, readRes) =>
    request('PATCH', `/api/favorites/${s}/${b}/${t}/progress`, { read_res: readRes }),
  setRating: (s, b, t, rating) =>
    request('PATCH', `/api/favorites/${s}/${b}/${t}/rating`, { rating }),
  search: (q) => request('GET', `/api/search?q=${encodeURIComponent(q)}`),
}

// ページ離脱時の確実な既読送信用（sendBeacon）。
export function beaconProgress(s, b, t, readRes) {
  const url = BASE + `/api/favorites/${s}/${b}/${t}/progress`
  const blob = new Blob([JSON.stringify({ read_res: readRes })], {
    type: 'application/json',
  })
  navigator.sendBeacon(url, blob)
}
