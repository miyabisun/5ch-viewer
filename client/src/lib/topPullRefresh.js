// topPullRefresh: Svelte action that detects a top pull-to-refresh gesture.
//
// Watches the window scroll and fires when the user drags DOWN from the very
// top of the page.
// Designed for the FavoritesList (window-scroll context).
//
// Gesture phases:
//   1. Arm  — only when scrollTop <= TOP_EPS (effectively at top of page)
//   2. Pull — downward drag (dy > 0) grows the pull panel
//   3. Release — if dy > PULL_THRESHOLD_PX, calls opts.onRefresh()
//
// Touch-only. Mouse/trackpad are not handled.
//
// opts:
//   enabled    — boolean: false disables the gesture entirely
//   isBlocked  — () => boolean: returns true when a modal is open
//   onRefresh  — () => void: called when the user releases past the threshold
//   onDrag     — (px: number) => void: current pull distance (0 when idle).
//                The host derives above-threshold state from px itself.

// Pull distance (px) above which releasing triggers a refresh.
const PULL_THRESHOLD_PX = 80

// scrollTop tolerance (px): arm the gesture when scrollTop is within this range.
const TOP_EPS = 2

// Maximum panel growth (rubber-band cap).
const PULL_MAX_PX = PULL_THRESHOLD_PX * 1.5

export function topPullRefresh(node, opts) {
  // opts may be a reactive getter (Svelte action pattern).
  function getOpts() {
    return typeof opts === 'function' ? opts() : opts
  }

  let startY = 0
  let armed = false   // true when touchstart fired at scrollTop <= TOP_EPS
  let ignore = false  // multi-touch → skip
  let pullPx = 0

  function collapse() {
    if (pullPx > 0) {
      pullPx = 0
      getOpts().onDrag(0)
    }
  }

  function onStart(e) {
    if (!getOpts().enabled) return
    // Ignore multi-touch.
    if (e.touches.length > 1) { ignore = true; collapse(); return }

    ignore = false
    armed = false
    pullPx = 0

    const scrollEl = document.scrollingElement
    if (!scrollEl) return

    // Arm only when at (or near) the top of the page.
    if (scrollEl.scrollTop <= TOP_EPS) {
      armed = true
      startY = e.touches[0].clientY
    }
  }

  function onMove(e) {
    if (ignore || !armed || !getOpts().enabled) return
    if (getOpts().isBlocked()) return

    const scrollEl = document.scrollingElement
    if (!scrollEl) return

    // If user scrolled down while dragging, disarm.
    if (scrollEl.scrollTop > TOP_EPS) {
      armed = false
      collapse()
      return
    }

    const dy = e.touches[0].clientY - startY

    // Only respond to downward drag.
    if (dy <= 0) {
      collapse()
      return
    }

    pullPx = Math.min(dy, PULL_MAX_PX)
    getOpts().onDrag(pullPx)
  }

  function onEnd() {
    if (ignore) return

    const o = getOpts()
    if (!armed || !o.enabled || o.isBlocked()) {
      armed = false
      collapse()
      return
    }

    if (pullPx >= PULL_THRESHOLD_PX) {
      o.onRefresh()
    }
    o.onDrag(0)
    armed = false
    pullPx = 0
  }

  node.addEventListener('touchstart', onStart, { passive: true })
  node.addEventListener('touchmove', onMove, { passive: true })
  node.addEventListener('touchend', onEnd, { passive: true })
  node.addEventListener('touchcancel', onEnd, { passive: true })

  return {
    destroy() {
      node.removeEventListener('touchstart', onStart)
      node.removeEventListener('touchmove', onMove)
      node.removeEventListener('touchend', onEnd)
      node.removeEventListener('touchcancel', onEnd)
    },
  }
}

export { PULL_THRESHOLD_PX }
