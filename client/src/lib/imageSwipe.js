// imageSwipe: Svelte action for left/right swipe navigation in ImageViewer.
//
// Touch-only. Horizontal 60px threshold triggers prev/next; vertical 100px closes.
// Direction is locked after the first 5px of travel (mirrors backSwipe / pullRefresh).
//
// opts (getter function or plain object):
//   onPrev    — () => void: called on right swipe (dx > 0 → go to previous image)
//   onNext    — () => void: called on left swipe  (dx < 0 → go to next image)
//   onClose   — () => void: called on downward swipe (close viewer)

const DIRECTION_LOCK_PX = 5
const HORIZONTAL_THRESHOLD_PX = 60
const VERTICAL_THRESHOLD_PX = 100

export function imageSwipe(node, opts) {
  function getOpts() {
    return typeof opts === 'function' ? opts() : opts
  }

  let startX = 0
  let startY = 0
  let locked = false
  let dir = null // 'h' | 'v' | null

  function onStart(e) {
    if (e.touches.length > 1) {
      locked = true
      dir = null
      return
    }
    const t = e.touches[0]
    startX = t.clientX
    startY = t.clientY
    locked = false
    dir = null
  }

  function onMove(e) {
    if (locked && dir == null) return
    const t = e.touches[0]
    const dx = t.clientX - startX
    const dy = t.clientY - startY
    if (!locked) {
      if (Math.abs(dx) < DIRECTION_LOCK_PX && Math.abs(dy) < DIRECTION_LOCK_PX) return
      locked = true
      dir = Math.abs(dx) >= Math.abs(dy) ? 'h' : 'v'
    }
  }

  function onEnd(e) {
    if (!locked || dir == null) return
    const t = e.changedTouches[0]
    const dx = t.clientX - startX
    const dy = t.clientY - startY
    const { onPrev, onNext, onClose } = getOpts()

    if (dir === 'h') {
      if (dx <= -HORIZONTAL_THRESHOLD_PX && onNext) onNext()
      else if (dx >= HORIZONTAL_THRESHOLD_PX && onPrev) onPrev()
    } else if (dir === 'v') {
      if (dy >= VERTICAL_THRESHOLD_PX && onClose) onClose()
    }

    locked = false
    dir = null
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
