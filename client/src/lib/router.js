// Minimal History API router. Maps the URL path to an app route and back.
// Routes:
//   /                              -> { page: 'favorites' }
//   /register                      -> { page: 'register' }
//   /{server}/{board}/{thread_id}  -> { page: 'favorites', thread: { server, board, thread_id } }
//
// Paths are resolved relative to BASE_PATH (the server may be mounted under a sub-path).

const BASE = (window.__BASE_PATH__ || '').replace(/\/$/, '')

// Strip the base prefix and return the leading-slash app path (e.g. "/egg/applism/123").
function appPath(pathname) {
  let p = pathname
  if (BASE && p.startsWith(BASE)) p = p.slice(BASE.length)
  if (!p.startsWith('/')) p = '/' + p
  return p
}

// Parse the current location into a route descriptor.
export function parseLocation() {
  const p = appPath(window.location.pathname)
  if (p === '/register') return { page: 'register', thread: null }
  if (p === '/archive') return { page: 'archive', thread: null }
  const m = p.match(/^\/([^/]+)\/([^/]+)\/([^/]+)\/?$/)
  if (m) {
    const [, server, board, thread_id] = m
    return { page: 'favorites', thread: { server, board, thread_id } }
  }
  // Root or any unknown path: fall back to favorites.
  return { page: 'favorites', thread: null }
}

// Build a URL path (with base prefix) from a route descriptor.
export function toPath({ page, thread }) {
  let p
  if (thread) p = `/${thread.server}/${thread.board}/${thread.thread_id}`
  else if (page === 'register') p = '/register'
  else if (page === 'archive') p = '/archive'
  else p = '/'
  return BASE + p
}

export function push(route) {
  history.pushState(null, '', toPath(route))
}

export function replace(route) {
  history.replaceState(null, '', toPath(route))
}
