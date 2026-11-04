// The single owner of the reading rules (docs/spec.md §8.1). Components only
// observe the screen and send requests; every decision about what is read,
// what is unread and what stays on screen is made here.
//
// - Read position (read_res): the highest res number that has been shown on
//   screen. Only showing a post advances it; opening, refreshing and posting
//   never do.
// - Unread: posts after the read position that are not the user's own.
// - The previous-read boundary and the unread marks use the read position at
//   entry and never move while the thread stays open.

/** Number of unread posts of a favorite row. */
export function unreadCount({ res_count, read_res, own_res = [] }) {
  const ownUnread = own_res.filter((n) => n > read_res && n <= res_count).length
  return Math.max(0, res_count - read_res - ownUnread)
}

/**
 * Newest-first timeline split into the posts to render now and the older posts
 * appended later in idle batches.
 *
 * On entry (`keepFrom` omitted) the rendered part ends at the previous-read
 * post. On a reload `keepFrom` is the oldest post already rendered, so new posts
 * are only added on top and nothing on screen is taken away.
 */
export function buildTimeline(res, baseline, keepFrom = null) {
  const newestFirst = res.toReversed()
  if (newestFirst.length === 0) return { visible: [], older: [], boundaryNum: null }

  // Include the previous-read post itself. A saved position beyond the
  // available dat falls back to the newest post.
  const found = newestFirst.findIndex((r) => r.num <= baseline)
  const boundaryIndex = baseline < 1 ? newestFirst.length - 1 : Math.max(0, found)
  let cutIndex = boundaryIndex
  if (keepFrom != null) {
    const lastKept = newestFirst.findLastIndex((r) => r.num >= keepFrom)
    cutIndex = Math.max(cutIndex, lastKept)
  }
  return {
    visible: newestFirst.slice(0, cutIndex + 1),
    older: newestFirst.slice(cutIndex + 1),
    boundaryNum: newestFirst[boundaryIndex].num,
  }
}

/**
 * The post to hold in place while the timeline changes: the topmost card that
 * reaches the viewport. `cards` are in DOM order with edges measured in the
 * scroller's coordinates; `viewTop` is the scroller's top edge.
 */
export function pickAnchor(cards, viewTop) {
  const card = cards.find((c) => c.bottom > viewTop)
  return card ? { num: card.num, offset: card.top - viewTop } : null
}
