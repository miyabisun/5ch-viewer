// pullRefresh: Svelte action that detects a bottom pull-to-refresh gesture (ChMate style).
//
// Gesture phases (mirrors ChMate):
//   1. Wall      — overscroll-behavior:contain stops native bounce; we enforce no extra scroll.
//   2. Lock      — 0.5 s at the bottom before the gesture is armed (prevents accidental trigger
//                  when the user arrives at the bottom via inertia/momentum).
//   3. Pull panel — after unlock, an upward swipe (dy < 0) slides a panel up from the bottom.
//                  Releasing past PULL_THRESHOLD_PX calls opts.onRefresh().
//
// Touch-only (mouse/trackpad are not handled).
//
// opts:
//   enabled    — boolean: false disables the gesture entirely (e.g. data not loaded, refreshing)
//   isBlocked  — () => boolean: returns true when a modal is open; gesture is suppressed
//   onRefresh  — () => void: called when the user releases past the threshold
//   onDrag     — (px: number, phase: 'dragging'|'idle') => void: called on each move/release
//                so ThreadView can animate the panel without owning gesture state

// Milliseconds at the bottom before the gesture is armed.
const UNLOCK_MS = 500

// Pull distance (px) above which releasing triggers a refresh.
const PULL_THRESHOLD_PX = 80

// Bottom-of-scroll tolerance in px to account for sub-pixel/zoom rounding.
const BOTTOM_EPS = 2

// Minimum travel (px) before we decide horizontal vs vertical direction.
const DIRECTION_LOCK_PX = 5

// Max pull overstretch: the panel does not grow beyond 1.5× the threshold.
// This gives a rubber-band feel and bounds the translateY range.
const PULL_MAX_PX = PULL_THRESHOLD_PX * 1.5

/** Returns the current scroll metrics for the thread body scroll container.
 *  Both PC and phone now use .thread-body as the sole scroll container (new layout). */
function getScrollState() {
  const body = document.querySelector('.thread-body')
  if (!body) return null
  return {
    scrollTop: body.scrollTop,
    clientHeight: body.clientHeight,
    scrollHeight: body.scrollHeight,
  }
}

/** True when the scroll container is at (or past) the very bottom. */
function isAtBottom() {
  const s = getScrollState()
  if (!s) return false
  // Guard: if content fits in one screen, never treat it as "at the bottom"
  // to avoid arming the gesture on short threads that cannot scroll at all.
  if (s.scrollHeight <= s.clientHeight + BOTTOM_EPS) return false
  return s.scrollTop + s.clientHeight >= s.scrollHeight - BOTTOM_EPS
}

export function pullRefresh(node, opts) {
  // opts is expected to be a reactive getter function so the action always
  // reads the latest values without needing update().
  function getOpts() {
    return typeof opts === 'function' ? opts() : opts
  }

  let startX = 0
  let startY = 0
  let locked = false    // direction lock acquired
  let vertical = false  // true = vertical gesture (not horizontal)
  let ignore = false    // multi-touch or anchor touch → skip

  // Unlock state: armed by a scroll event that ends at the bottom and
  // confirmed after UNLOCK_MS of staying there. Cleared when leaving the bottom.
  let unlocked = false
  let unlockTimer = null

  // Pull amount while dragging (px, positive = panel visible).
  let pullPx = 0

  function clearUnlock() {
    clearTimeout(unlockTimer)
    unlockTimer = null
    unlocked = false
  }

  // Collapse the panel back to hidden, notifying the host once (only if visible).
  function collapse() {
    if (pullPx > 0) {
      pullPx = 0
      getOpts().onDrag(0, 'idle')
    }
  }

  function armUnlock() {
    if (unlocked || unlockTimer != null) return
    unlockTimer = setTimeout(() => {
      unlocked = true
      unlockTimer = null
    }, UNLOCK_MS)
  }

  // Watch the scroll container: when the user scrolls to the bottom and stays
  // there for UNLOCK_MS, the gesture is armed. Leaving the bottom resets it.
  // This fires while the finger is still scrolling (touchstart has not come yet),
  // which matches ChMate: the "hold at bottom" phase is measured from when
  // the scroll position reaches the bottom, not from the next touchstart.
  function onScroll() {
    if (!getOpts().enabled) return
    if (isAtBottom()) {
      armUnlock()
    } else {
      clearUnlock()
    }
  }

  function onStart(e) {
    if (!getOpts().enabled) return
    // Ignore multi-touch (pinch/zoom).
    if (e.touches.length > 1) { ignore = true; return }

    ignore = false
    const t = e.touches[0]
    startX = t.clientX
    startY = t.clientY
    locked = false
    vertical = false
    pullPx = 0

    // Also check at touchstart in case scroll event was not fired
    // (e.g. content was already at the bottom before any scroll).
    if (isAtBottom()) {
      armUnlock()
    }
  }

  function onMove(e) {
    if (ignore || !getOpts().enabled) return
    if (getOpts().isBlocked()) return

    const t = e.touches[0]
    const dx = t.clientX - startX
    const dy = t.clientY - startY

    // If we moved away from bottom while dragging, collapse panel and bail.
    if (!isAtBottom()) {
      clearUnlock()
      collapse()
      return
    }

    // Direction lock: wait for DIRECTION_LOCK_PX of travel before deciding.
    if (!locked) {
      if (Math.abs(dx) < DIRECTION_LOCK_PX && Math.abs(dy) < DIRECTION_LOCK_PX) return
      locked = true
      vertical = Math.abs(dy) > Math.abs(dx)
    }

    if (!vertical) return  // horizontal gesture → let backSwipe handle it
    if (!unlocked) return  // still in the 0.5 s lock period

    // Upward swipe at the bottom = over-pull (dy < 0 means finger moves up).
    if (dy >= 0) {
      collapse()
      return
    }

    // Clamp pull amount to PULL_MAX_PX for a rubber-band feel.
    pullPx = Math.min(-dy, PULL_MAX_PX)
    getOpts().onDrag(pullPx, 'dragging')
  }

  function onEnd() {
    if (ignore) return

    const { enabled, isBlocked, onRefresh, onDrag } = getOpts()
    if (!enabled || isBlocked()) {
      reset()
      return
    }

    if (pullPx >= PULL_THRESHOLD_PX) {
      onRefresh()
    }
    // Always collapse the panel on finger-up, regardless of threshold.
    onDrag(0, 'idle')
    reset()
  }

  function reset() {
    pullPx = 0
    locked = false
    vertical = false
    ignore = false
  }

  // Attach the scroll listener to .thread-body, which is the sole scroll container
  // in the new layout (both PC and phone scroll inside .thread-body).
  const scrollTarget = document.querySelector('.thread-body') ?? window

  scrollTarget.addEventListener('scroll', onScroll, { passive: true })
  node.addEventListener('touchstart', onStart, { passive: true })
  node.addEventListener('touchmove', onMove, { passive: true })
  node.addEventListener('touchend', onEnd, { passive: true })
  node.addEventListener('touchcancel', onEnd, { passive: true })

  return {
    destroy() {
      clearUnlock()
      scrollTarget.removeEventListener('scroll', onScroll)
      node.removeEventListener('touchstart', onStart)
      node.removeEventListener('touchmove', onMove)
      node.removeEventListener('touchend', onEnd)
      node.removeEventListener('touchcancel', onEnd)
    },
  }
}

export { PULL_THRESHOLD_PX, UNLOCK_MS, BOTTOM_EPS }
