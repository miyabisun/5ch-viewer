// Theme management: OS-follow by default, manual override saved to localStorage.
const KEY = '5ch-theme'

// Resolve the effective theme: stored override, or OS preference.
function resolve() {
  const saved = localStorage.getItem(KEY)
  if (saved === 'light' || saved === 'dark') return saved
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

// Composite a computed rgb()/rgba() colour onto an opaque one. A translucent
// theme-color loses what shows through it, so an overlay reports the colour seen.
export function opaqueOver(top, beneath) {
  const [r, g, b, a = 1] = channels(top)
  const [br, bg, bb] = channels(beneath)
  const mix = (over, under) => Math.round(over * a + under * (1 - a))
  return `rgb(${mix(r, br)}, ${mix(g, bg)}, ${mix(b, bb)})`
}

function channels(color) {
  return color.match(/[\d.]+/g).map(Number)
}

// Mirror what fills the screen into the page's <meta name="theme-color">: the page
// background, or `layer` while a full-screen overlay covers it. Chrome colours a tab's
// system bars from it. An installed app uses the standalone meta in index.html instead,
// since its buttons sit over the fixed manifest backdrop. The CSS tokens stay the source.
export function syncThemeColor(layer = document.body) {
  let meta = document.querySelector('meta[name="theme-color"]:not([media])')
  if (!meta) {
    meta = document.createElement('meta')
    meta.name = 'theme-color'
    document.head.append(meta)
  }
  const page = getComputedStyle(document.body).backgroundColor
  meta.content = opaqueOver(getComputedStyle(layer).backgroundColor, page)
}

function apply(theme) {
  document.documentElement.dataset.theme = theme
  syncThemeColor()
}

// Apply on load (no override stored => follow OS).
export function initTheme() {
  apply(resolve())
}

export function currentTheme() {
  return resolve()
}

// Toggle and persist the override.
export function toggleTheme() {
  const next = resolve() === 'dark' ? 'light' : 'dark'
  localStorage.setItem(KEY, next)
  apply(next)
  return next
}
