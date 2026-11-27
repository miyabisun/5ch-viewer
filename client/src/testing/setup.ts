// What every test of the client runs in: jsdom, with the few Web APIs the page calls that
// jsdom lacks, made anew for each test. They do as little as the page needs; what a test
// looks at, it sets itself (page.ts). Nothing heavy is loaded here: a test of a pure function
// pays for none of it (the testing library comes with page.ts).
import { afterEach, beforeEach, vi } from 'vitest'

/** The observers the page made, each with the elements it watches. */
export const observers = new Set<FakeIntersectionObserver>()

/**
 * An IntersectionObserver that tells nothing by itself: jsdom lays nothing out, so nothing is
 * in sight until a test says so (`seen` in page.ts), as the browser would on a scroll.
 */
class FakeIntersectionObserver {
  watched = new Set<Element>()
  constructor(public callback: IntersectionObserverCallback) {
    observers.add(this)
  }
  observe(element: Element) {
    this.watched.add(element)
  }
  unobserve(element: Element) {
    this.watched.delete(element)
  }
  disconnect() {
    this.watched.clear()
  }
  takeRecords() {
    return []
  }
}

/** What `navigator.sendBeacon` sent, for the fake server to answer as a POST (server.ts). */
export const beacons: { url: string; body: Blob }[] = []

beforeEach(() => {
  observers.clear()
  beacons.length = 0
  vi.stubGlobal('IntersectionObserver', FakeIntersectionObserver)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  // the system's colour scheme is light, unless a test says otherwise
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }))
  Element.prototype.scrollIntoView = () => {}
  Element.prototype.scrollTo = () => {}
  Element.prototype.scrollBy = () => {}
  window.scrollTo = () => {}
  Element.prototype.setPointerCapture = () => {}
  Element.prototype.releasePointerCapture = () => {}
  Element.prototype.hasPointerCapture = () => false
  Object.defineProperty(navigator, 'sendBeacon', {
    configurable: true,
    value: (url: string, body: Blob) => {
      beacons.push({ url, body })
      return true
    },
  })
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: vi.fn(() => Promise.resolve()) },
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  vi.useRealTimers()
  localStorage.clear()
  sessionStorage.clear()
  history.replaceState(null, '', '/')
  delete document.documentElement.dataset.theme
})
