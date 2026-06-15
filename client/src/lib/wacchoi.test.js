import { describe, it, expect } from 'vitest'
import { extractWacchoi, wacchoiEnabled, buildWacchoiStats } from './wacchoi.js'

// ---------------------------------------------------------------------------
// extractWacchoi
// ---------------------------------------------------------------------------
describe('extractWacchoi', () => {
  it('extracts wacchoi from a raw name with HTML tags', () => {
    // Actual name field as stored in dat: tags must be handled by formatName.
    const raw = 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>'
    expect(extractWacchoi(raw)).toBe('7bb6-83IP')
  })

  it('extracts wacchoi from an already-formatted name', () => {
    const formatted = 'iPhone774G (ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])'
    expect(extractWacchoi(formatted)).toBe('7bb6-83IP')
  })

  it('extracts wacchoi from the example in the task spec', () => {
    // Spec example: "826 iPhone774G (ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*]) 2026/06/15(月) 00:53:01.03"
    // In name field only the name portion appears, not the number or date.
    const name = 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>'
    expect(extractWacchoi(name)).toBe('7bb6-83IP')
  })

  it('returns null when no parenthesised wacchoi token is present', () => {
    expect(extractWacchoi('名無しさん')).toBeNull()
  })

  it('returns null for empty string', () => {
    expect(extractWacchoi('')).toBeNull()
  })

  it('returns null for null', () => {
    expect(extractWacchoi(null)).toBeNull()
  })

  it('returns null for undefined', () => {
    expect(extractWacchoi(undefined)).toBeNull()
  })

  it('takes the first \\w{4}-\\w{4} in case of multiple parenthesised groups', () => {
    // e.g. "(ﾜｯﾁｮｲ aabb-1234 ...) (ﾜﾝｸﾛ ccdd-5678 ...)"
    const name = 'foo (ﾜｯﾁｮｲ aabb-1234 [::]) (ﾜﾝｸﾛ ccdd-5678 [::])'
    expect(extractWacchoi(name)).toBe('aabb-1234')
  })

  it('is not confused by IPv6 brackets following the token', () => {
    // Word-boundary anchors should stop matching before the colon-separated IPv6 part.
    const name = 'foo (ﾜｯﾁｮｲ AbC1-dEf2 [2001:db8::])'
    expect(extractWacchoi(name)).toBe('AbC1-dEf2')
  })

  it('handles a name containing parentheses but no wacchoi pattern', () => {
    expect(extractWacchoi('名無しさん (IP有り)')).toBeNull()
  })

  it('extracts wacchoi from another common format (b6f1-daVb)', () => {
    const raw = 'iPhone774G </b>(ﾜｯﾁｮｲ b6f1-daVb [2001:268:77aa:c6e4:*])<b>'
    expect(extractWacchoi(raw)).toBe('b6f1-daVb')
  })
})

// ---------------------------------------------------------------------------
// wacchoiEnabled
// ---------------------------------------------------------------------------
describe('wacchoiEnabled', () => {
  function makeRes(num, name) {
    return { num, name, date: '', body: '' }
  }

  it('returns true when res num=1 contains a wacchoi', () => {
    const res = [makeRes(1, 'foo </b>(ﾜｯﾁｮｲ 1234-abcd [::1])<b>')]
    expect(wacchoiEnabled(res)).toBe(true)
  })

  it('returns false when res num=1 has no wacchoi', () => {
    const res = [makeRes(1, '名無しさん')]
    expect(wacchoiEnabled(res)).toBe(false)
  })

  it('uses the array head when no res has num===1', () => {
    // Edge case: partial thread load starting from res 5.
    const res = [
      makeRes(5, 'foo </b>(ﾜｯﾁｮｲ 1234-abcd [::1])<b>'),
      makeRes(6, '名無しさん'),
    ]
    expect(wacchoiEnabled(res)).toBe(true)
  })

  it('returns false for empty list', () => {
    expect(wacchoiEnabled([])).toBe(false)
  })

  it('returns false for null', () => {
    expect(wacchoiEnabled(null)).toBe(false)
  })
})

// ---------------------------------------------------------------------------
// buildWacchoiStats
// ---------------------------------------------------------------------------
describe('buildWacchoiStats', () => {
  function makeRes(num, name) {
    return { num, name, date: '', body: '' }
  }

  it('returns empty Map when enabled=false regardless of input', () => {
    const res = [makeRes(1, 'foo </b>(ﾜｯﾁｮｲ 1234-abcd [::1])<b>')]
    expect(buildWacchoiStats(res, false)).toEqual(new Map())
  })

  it('returns empty Map for empty list even when enabled=true', () => {
    expect(buildWacchoiStats([], true)).toEqual(new Map())
  })

  it('single res with wacchoi -> total=1, order=1, colorLevel=none', () => {
    const res = [makeRes(1, 'foo </b>(ﾜｯﾁｮｲ 1111-aaaa [::1])<b>')]
    const stats = buildWacchoiStats(res, true)
    expect(stats.get(1)).toEqual({
      wacchoi: '1111-aaaa',
      total: 1,
      order: 1,
      colorLevel: 'none',
    })
  })

  it('two reses with same wacchoi -> total=2, orders 1/2, colorLevel=l2', () => {
    const res = [
      makeRes(1, 'foo </b>(ﾜｯﾁｮｲ aaaa-1111 [::1])<b>'),
      makeRes(2, 'foo </b>(ﾜｯﾁｮｲ aaaa-1111 [::1])<b>'),
    ]
    const stats = buildWacchoiStats(res, true)
    expect(stats.get(1)).toMatchObject({ total: 2, order: 1, colorLevel: 'l2' })
    expect(stats.get(2)).toMatchObject({ total: 2, order: 2, colorLevel: 'l2' })
  })

  it('res without wacchoi gets no entry in the map', () => {
    const res = [
      makeRes(1, '名無しさん'),
      makeRes(2, 'bar </b>(ﾜｯﾁｮｲ bbbb-2222 [::1])<b>'),
    ]
    const stats = buildWacchoiStats(res, true)
    expect(stats.has(1)).toBe(false)
    expect(stats.has(2)).toBe(true)
  })

  it('mixed wacchois are counted independently', () => {
    const res = [
      makeRes(1, 'a </b>(ﾜｯﾁｮｲ AAAA-1111 [::1])<b>'),
      makeRes(2, 'b </b>(ﾜｯﾁｮｲ BBBB-2222 [::1])<b>'),
      makeRes(3, 'a </b>(ﾜｯﾁｮｲ AAAA-1111 [::1])<b>'),
    ]
    const stats = buildWacchoiStats(res, true)
    expect(stats.get(1)).toMatchObject({ wacchoi: 'AAAA-1111', total: 2, order: 1 })
    expect(stats.get(3)).toMatchObject({ wacchoi: 'AAAA-1111', total: 2, order: 2 })
    expect(stats.get(2)).toMatchObject({ wacchoi: 'BBBB-2222', total: 1, order: 1 })
  })

  // -------------------------------------------------------------------------
  // colorLevel boundary tests (mirrors id.test.js)
  // -------------------------------------------------------------------------
  function manyRes(n) {
    return Array.from({ length: n }, (_, i) =>
      makeRes(i + 1, `foo </b>(ﾜｯﾁｮｲ same-WACH [::${i}])<b>`),
    )
  }

  it('total=1 -> colorLevel none', () => {
    const stats = buildWacchoiStats(manyRes(1), true)
    expect(stats.get(1)?.colorLevel).toBe('none')
  })

  it('total=2 -> colorLevel l2', () => {
    const stats = buildWacchoiStats(manyRes(2), true)
    expect(stats.get(1)?.colorLevel).toBe('l2')
  })

  it('total=3 -> colorLevel l2', () => {
    const stats = buildWacchoiStats(manyRes(3), true)
    expect(stats.get(1)?.colorLevel).toBe('l2')
  })

  it('total=4 -> colorLevel l3', () => {
    const stats = buildWacchoiStats(manyRes(4), true)
    expect(stats.get(1)?.colorLevel).toBe('l3')
  })

  it('total=6 -> colorLevel l3', () => {
    const stats = buildWacchoiStats(manyRes(6), true)
    expect(stats.get(1)?.colorLevel).toBe('l3')
  })

  it('total=7 -> colorLevel l4', () => {
    const stats = buildWacchoiStats(manyRes(7), true)
    expect(stats.get(1)?.colorLevel).toBe('l4')
  })

  it('total=9 -> colorLevel l4', () => {
    const stats = buildWacchoiStats(manyRes(9), true)
    expect(stats.get(1)?.colorLevel).toBe('l4')
  })

  it('total=10 -> colorLevel l5', () => {
    const stats = buildWacchoiStats(manyRes(10), true)
    expect(stats.get(1)?.colorLevel).toBe('l5')
  })

  it('total=15 -> colorLevel l5', () => {
    const stats = buildWacchoiStats(manyRes(15), true)
    expect(stats.get(1)?.colorLevel).toBe('l5')
  })
})
