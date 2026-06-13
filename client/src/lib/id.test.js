import { describe, it, expect } from 'vitest'
import { extractId, stripId, buildIdStats } from './id.js'

// ---------------------------------------------------------------------------
// extractId
// ---------------------------------------------------------------------------
describe('extractId', () => {
  it('extracts the ID key from a typical date string', () => {
    expect(extractId('2025/01/01(水) 12:34:56.78 ID:klSUPSuq0')).toBe(
      'klSUPSuq0',
    )
  })

  it('returns null when no ID token is present', () => {
    expect(extractId('2025/01/01(水) 12:34:56.78')).toBeNull()
  })

  it('returns null for empty string', () => {
    expect(extractId('')).toBeNull()
  })

  it('returns null for null/undefined', () => {
    expect(extractId(null)).toBeNull()
    expect(extractId(undefined)).toBeNull()
  })

  it('extracts ID when it appears at the end of the string (no trailing space)', () => {
    expect(extractId('2025/01/01(水) 12:34:56.78 ID:AbC123')).toBe('AbC123')
  })

  it('extracts only the first ID token (stops at next whitespace)', () => {
    // Unusual but defensive: only the first match is taken
    expect(extractId('2025/01/01 12:00:00 ID:first0 ID:second')).toBe('first0')
  })

  it('handles IDs with symbols (e.g. + and /)', () => {
    expect(extractId('2025/06/01(月) 00:00:00.00 ID:a+b/c==')).toBe('a+b/c==')
  })
})

// ---------------------------------------------------------------------------
// stripId
// ---------------------------------------------------------------------------
describe('stripId', () => {
  it('removes the ID token and the preceding space', () => {
    expect(stripId('2025/01/01(水) 12:34:56.78 ID:klSUPSuq0')).toBe(
      '2025/01/01(水) 12:34:56.78',
    )
  })

  it('leaves a date without an ID unchanged (trim only)', () => {
    expect(stripId('2025/01/01(水) 12:34:56.78')).toBe(
      '2025/01/01(水) 12:34:56.78',
    )
  })

  it('returns empty string for empty/nullish input', () => {
    expect(stripId('')).toBe('')
    expect(stripId(null)).toBe('')
    expect(stripId(undefined)).toBe('')
  })
})

// ---------------------------------------------------------------------------
// buildIdStats
// ---------------------------------------------------------------------------
describe('buildIdStats', () => {
  function makeRes(num, date) {
    return { num, date, name: '', body: '' }
  }

  it('returns an empty map for an empty res list', () => {
    expect(buildIdStats([])).toEqual(new Map())
  })

  it('single res with ID -> total=1, order=1, colorLevel=none', () => {
    const stats = buildIdStats([makeRes(1, '2025/01/01 00:00:00 ID:aaa')])
    expect(stats.get(1)).toEqual({ id: 'aaa', total: 1, order: 1, colorLevel: 'none' })
  })

  it('two reses with same ID -> total=2, orders 1 and 2, colorLevel=l2', () => {
    const stats = buildIdStats([
      makeRes(1, '2025/01/01 00:00:00 ID:aaa'),
      makeRes(2, '2025/01/01 00:01:00 ID:aaa'),
    ])
    expect(stats.get(1)).toMatchObject({ total: 2, order: 1, colorLevel: 'l2' })
    expect(stats.get(2)).toMatchObject({ total: 2, order: 2, colorLevel: 'l2' })
  })

  it('res without ID gets no entry in the map', () => {
    const stats = buildIdStats([
      makeRes(1, '2025/01/01 00:00:00'),
      makeRes(2, '2025/01/01 00:01:00 ID:bbb'),
    ])
    expect(stats.has(1)).toBe(false)
    expect(stats.has(2)).toBe(true)
  })

  it('mixed IDs are counted independently', () => {
    const stats = buildIdStats([
      makeRes(1, '2025/01/01 00:00:00 ID:AAA'),
      makeRes(2, '2025/01/01 00:01:00 ID:BBB'),
      makeRes(3, '2025/01/01 00:02:00 ID:AAA'),
    ])
    expect(stats.get(1)).toMatchObject({ id: 'AAA', total: 2, order: 1 })
    expect(stats.get(3)).toMatchObject({ id: 'AAA', total: 2, order: 2 })
    expect(stats.get(2)).toMatchObject({ id: 'BBB', total: 1, order: 1 })
  })

  // -------------------------------------------------------------------------
  // colorLevel boundary tests (off-by-one coverage)
  // -------------------------------------------------------------------------
  function manyRes(n) {
    return Array.from({ length: n }, (_, i) =>
      makeRes(i + 1, `2025/01/01 00:0${i}:00 ID:sameID`),
    )
  }

  it('total=1 -> colorLevel none', () => {
    const stats = buildIdStats(manyRes(1))
    expect(stats.get(1)?.colorLevel).toBe('none')
  })

  it('total=2 -> colorLevel l2', () => {
    const stats = buildIdStats(manyRes(2))
    expect(stats.get(1)?.colorLevel).toBe('l2')
  })

  it('total=3 -> colorLevel l2', () => {
    const stats = buildIdStats(manyRes(3))
    expect(stats.get(1)?.colorLevel).toBe('l2')
  })

  it('total=4 -> colorLevel l3', () => {
    const stats = buildIdStats(manyRes(4))
    expect(stats.get(1)?.colorLevel).toBe('l3')
  })

  it('total=6 -> colorLevel l3', () => {
    const stats = buildIdStats(manyRes(6))
    expect(stats.get(1)?.colorLevel).toBe('l3')
  })

  it('total=7 -> colorLevel l4', () => {
    const stats = buildIdStats(manyRes(7))
    expect(stats.get(1)?.colorLevel).toBe('l4')
  })

  it('total=9 -> colorLevel l4', () => {
    const stats = buildIdStats(manyRes(9))
    expect(stats.get(1)?.colorLevel).toBe('l4')
  })

  it('total=10 -> colorLevel l5', () => {
    const stats = buildIdStats(manyRes(10))
    expect(stats.get(1)?.colorLevel).toBe('l5')
  })

  it('total=15 -> colorLevel l5', () => {
    const stats = buildIdStats(manyRes(15))
    expect(stats.get(1)?.colorLevel).toBe('l5')
  })
})
