import { describe, expect, it } from 'vitest'
import { buildTimeline, pickAnchor, unreadCount } from './read-progress.js'

const posts = (count) => Array.from({ length: count }, (_, i) => ({ num: i + 1 }))
const nums = (list) => list.map((r) => r.num)
const range = (from, to) => {
  const out = []
  for (let n = from; from <= to ? n <= to : n >= to; n += from <= to ? 1 : -1) out.push(n)
  return out
}

describe('unreadCount', () => {
  it('counts the posts after the read position', () => {
    expect(unreadCount({ res_count: 30, read_res: 28 })).toBe(2)
  })

  it('does not count the user’s own posts after the read position', () => {
    expect(unreadCount({ res_count: 61, read_res: 40, own_res: [61] })).toBe(20)
    expect(unreadCount({ res_count: 61, read_res: 60, own_res: [61] })).toBe(0)
  })

  it('ignores own posts already read or not yet in the saved dat', () => {
    expect(unreadCount({ res_count: 50, read_res: 40, own_res: [12, 40, 51] })).toBe(10)
  })

  it('never goes below zero when the read position is ahead of the dat', () => {
    expect(unreadCount({ res_count: 30, read_res: 50, own_res: [] })).toBe(0)
  })
})

describe('buildTimeline', () => {
  it('opens newest-first down to the previous-read post', () => {
    const t = buildTimeline(posts(120), 100)
    expect(t.boundaryNum).toBe(100)
    expect(nums(t.visible)).toEqual(range(120, 100))
    expect(nums(t.older)).toEqual(range(99, 1))
  })

  it('places the boundary after res 1 when nothing has been read', () => {
    const t = buildTimeline(posts(5), 0)
    expect(t.boundaryNum).toBe(1)
    expect(nums(t.visible)).toEqual([5, 4, 3, 2, 1])
    expect(t.older).toEqual([])
  })

  it('falls back to the newest post when the saved position is beyond the dat', () => {
    const t = buildTimeline(posts(30), 50)
    expect(t.boundaryNum).toBe(30)
    expect(nums(t.visible)).toEqual([30])
  })

  it('keeps every post already rendered when newer posts arrive', () => {
    // Opened at read 100 and scrolled down to res 50; a reload brings res 121.
    const t = buildTimeline(posts(121), 100, 50)
    expect(t.boundaryNum).toBe(100)
    expect(nums(t.visible)).toEqual(range(121, 50))
    expect(nums(t.older)).toEqual(range(49, 1))
  })

  it('never cuts above the previous-read post', () => {
    const t = buildTimeline(posts(121), 100, 110)
    expect(nums(t.visible)).toEqual(range(121, 100))
  })

  it('is empty for an empty dat', () => {
    expect(buildTimeline([], 10)).toEqual({ visible: [], older: [], boundaryNum: null })
  })
})

describe('pickAnchor', () => {
  // Cards in DOM order (newest first) with their edges relative to the scroller.
  const cards = [
    { num: 61, top: -300, bottom: -200 },
    { num: 60, top: -200, bottom: 0 },
    { num: 59, top: 0, bottom: 120 },
    { num: 58, top: 120, bottom: 400 },
  ]

  it('keeps the topmost post that is on screen', () => {
    expect(pickAnchor(cards, 0)).toEqual({ num: 59, offset: 0 })
    expect(pickAnchor(cards, 50)).toEqual({ num: 59, offset: -50 })
  })

  it('returns null when no post reaches the viewport', () => {
    expect(pickAnchor(cards, 500)).toBeNull()
    expect(pickAnchor([], 0)).toBeNull()
  })
})
