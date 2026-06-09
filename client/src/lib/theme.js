// Theme management: OS-follow by default, manual override saved to localStorage.
const KEY = 'goch-theme'

// Resolve the effective theme: stored override, or OS preference.
function resolve() {
  const saved = localStorage.getItem(KEY)
  if (saved === 'light' || saved === 'dark') return saved
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function apply(theme) {
  document.documentElement.dataset.theme = theme
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
