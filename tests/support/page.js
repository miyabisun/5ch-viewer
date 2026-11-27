// What every document of the page runs first, for the page tests (tests/page.rs).
//
// The tests that need data of a given shape (wacchoi names, long IDs, many posts) answer the
// page's fetches of /api/* from `window.__routes`, which tests/page.rs sets before the page
// opens, as the jsdom tests do with their fake server: nothing reaches the app for those. With
// no routes set, fetch goes to the app (the real router on an in-memory database).
// `window.t` holds the finders and measures the tests use.
;(() => {
  const real = window.fetch.bind(window)
  window.__asked = []
  window.fetch = async (input, init = {}) => {
    const url = new URL(String(input instanceof Request ? input.url : input), location.href)
    const method = (init.method || 'GET').toUpperCase()
    const routes = window.__routes
    if (!routes || !url.pathname.startsWith('/api/')) return real(input, init)
    window.__asked.push({ method, path: url.pathname })
    const route = routes.find(
      (one) =>
        (!one.method || one.method === method) &&
        (one.path ? one.path === url.pathname : new RegExp(one.pattern).test(url.pathname)),
    )
    let answer = route ?? { status: 404, json: { error: 'no route' } }
    if (route?.answer) {
      const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined
      answer = { json: route.answer({ method, path: url.pathname, body }) }
    }
    return new Response(JSON.stringify(answer.json ?? null), {
      status: answer.status ?? 200,
      headers: { 'content-type': 'application/json' },
    })
  }

  // a beacon is a POST whose answer nobody reads: with routes set, the routes answer it
  const beacon = navigator.sendBeacon.bind(navigator)
  navigator.sendBeacon = (url, data) => {
    if (!window.__routes) return beacon(url, data)
    Promise.resolve(data instanceof Blob ? data.text() : String(data)).then((body) =>
      window.fetch(url, { method: 'POST', body }),
    )
    return true
  }

  window.t = {
    /** The first element `selector` finds, or null. */
    $: (selector) => document.querySelector(selector),
    /** The element whose own text is `text`, whole. */
    text(text) {
      return (
        [...document.body.querySelectorAll('*')].find(
          (el) => el.textContent.trim() === text && ![...el.children].some((c) => c.textContent.trim() === text),
        ) ?? null
      )
    },
    /** Where `el` lies in the window: [left, top, width, height, right, bottom]. */
    box(el) {
      const r = el.getBoundingClientRect()
      return [r.left, r.top, r.width, r.height, r.right, r.bottom]
    },
    /** Whether `el` takes room on the screen and is not hidden. */
    shows(el) {
      return !!el && el.checkVisibility({ visibilityProperty: true }) && el.getClientRects().length > 0
    },
  }
})()
