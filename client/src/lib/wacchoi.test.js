import { describe, it, expect } from 'vitest'
import { extractWacchoi, wacchoiEnabled, buildWacchoiStats, linkifyWacchoi, extractWacchoiSuffix, wacchoiWeekKey } from './wacchoi.js'

// ---------------------------------------------------------------------------
// linkifyWacchoi
// ---------------------------------------------------------------------------
describe('linkifyWacchoi', () => {
  it('wraps the wacchoi token in a .wacchoi-badge span', () => {
    const raw = 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>'
    const result = linkifyWacchoi(raw)
    expect(result).toContain(
      '<span class="wacchoi-badge" data-wacchoi="7bb6-83IP">7bb6-83IP</span>',
    )
  })

  it('preserves surrounding text (ﾜｯﾁｮｲ prefix and IP address)', () => {
    const raw = 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>'
    const result = linkifyWacchoi(raw)
    expect(result).toContain('ﾜｯﾁｮｲ')
    expect(result).toContain('2400:4050')
    expect(result).toContain('iPhone774G')
  })

  it('strips HTML tags from name (XSS prevention via formatName)', () => {
    // formatName() strips ALL tags before escapeHtml runs, so <script> disappears
    // entirely rather than being escaped — the result is safe for {@html}.
    const raw = '<script>alert(1)</script> </b>(ﾜｯﾁｮｲ 7bb6-83IP [::1])<b>'
    const result = linkifyWacchoi(raw)
    expect(result).not.toContain('<script>')
    // The text content between tags ("alert(1)") survives, but the tag itself is gone.
    expect(result).toContain('alert(1)')
    // The wacchoi span is still present.
    expect(result).toContain('<span class="wacchoi-badge" data-wacchoi="7bb6-83IP">7bb6-83IP</span>')
  })

  it('HTML-escapes double quotes in the name (XSS prevention)', () => {
    const raw = 'foo"bar </b>(ﾜｯﾁｮｲ 7bb6-83IP [::1])<b>'
    const result = linkifyWacchoi(raw)
    expect(result).toContain('foo&quot;bar')
  })

  it('data-wacchoi attribute is intact and not broken by escaping', () => {
    const raw = 'name </b>(ﾜｯﾁｮｲ Ab12-Cd34 [::1])<b>'
    const result = linkifyWacchoi(raw)
    expect(result).toContain('data-wacchoi="Ab12-Cd34"')
  })

  it('returns escaped plain text when there is no wacchoi token', () => {
    const result = linkifyWacchoi('名無しさん')
    expect(result).toBe('名無しさん')
    expect(result).not.toContain('<span')
  })

  it('returns escaped plain text for a name with parentheses but no wacchoi', () => {
    const result = linkifyWacchoi('名無しさん (IP有り)')
    expect(result).toBe('名無しさん (IP有り)')
    expect(result).not.toContain('<span')
  })

  it('returns empty string for empty string', () => {
    expect(linkifyWacchoi('')).toBe('')
  })

  it('returns empty string for null', () => {
    expect(linkifyWacchoi(null)).toBe('')
  })

  it('returns empty string for undefined', () => {
    expect(linkifyWacchoi(undefined)).toBe('')
  })

  it('replaces only the first wacchoi token when the name has multiple \\w{4}-\\w{4} patterns', () => {
    // extractWacchoi picks the first parenthesised token; linkifyWacchoi must do the same.
    const raw = 'foo (ﾜｯﾁｮｲ aabb-1234 [::]) (ﾜﾝｸﾛ ccdd-5678 [::])'
    const result = linkifyWacchoi(raw)
    // First token is span-ified.
    expect(result).toContain('<span class="wacchoi-badge" data-wacchoi="aabb-1234">aabb-1234</span>')
    // Second token must remain plain text (not wrapped).
    expect(result).not.toContain('data-wacchoi="ccdd-5678"')
    expect(result).toContain('ccdd-5678')
  })

  it('handles ampersand in name without double-escaping', () => {
    const result = linkifyWacchoi('A&B (ﾜｯﾁｮｲ 1234-abcd [::1])')
    expect(result).toContain('A&amp;B')
    expect(result).not.toContain('&amp;amp;')
  })
})

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

// ---------------------------------------------------------------------------
// extractWacchoiSuffix
// ---------------------------------------------------------------------------
describe('extractWacchoiSuffix', () => {
  it('extracts suffix from a full name string (with HTML tags)', () => {
    const raw = 'iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400::])<b>'
    expect(extractWacchoiSuffix(raw)).toBe('83IP')
  })

  it('extracts suffix from a bare token (no parens)', () => {
    // Already-extracted token like '7bb6-83IP' — common when coming from data-wacchoi.
    expect(extractWacchoiSuffix('7bb6-83IP')).toBe('83IP')
  })

  it('extracts suffix from a formatted name (no HTML tags)', () => {
    const formatted = 'iPhone774G (ﾜｯﾁｮｲ Ab12-Cd34 [::1])'
    expect(extractWacchoiSuffix(formatted)).toBe('Cd34')
  })

  it('returns null for a bare token where the suffix is not 4 chars', () => {
    // 'abc-12' -> suffix '12' (only 2 chars) -> null
    expect(extractWacchoiSuffix('abc-12')).toBeNull()
    // 'abcd-12345' -> suffix '12345' (5 chars) -> null
    expect(extractWacchoiSuffix('abcd-12345')).toBeNull()
  })

  it('returns null for a bare token with no hyphen', () => {
    expect(extractWacchoiSuffix('nohyphen')).toBeNull()
  })

  it('returns null when there is no wacchoi token in a full name', () => {
    expect(extractWacchoiSuffix('名無しさん')).toBeNull()
  })

  it('returns null for null input', () => {
    expect(extractWacchoiSuffix(null)).toBeNull()
  })

  it('returns null for empty string', () => {
    expect(extractWacchoiSuffix('')).toBeNull()
  })
})

// ---------------------------------------------------------------------------
// wacchoiWeekKey
// ---------------------------------------------------------------------------
describe('wacchoiWeekKey', () => {
  // Thursday itself -> same day is the week start.
  it('Thursday date returns itself as the week key', () => {
    // 2025/01/09 is a Thursday.
    expect(wacchoiWeekKey('2025/01/09(木) 00:00:00.00')).toBe('2025/01/09')
  })

  // Wednesday just before next Thursday -> falls back to the previous Thursday.
  it('Wednesday date returns the preceding Thursday', () => {
    // 2025/01/08(水) -> previous Thursday was 2025/01/02.
    expect(wacchoiWeekKey('2025/01/08(水) 23:59:59.99')).toBe('2025/01/02')
  })

  // The same Thursday → week key must differ from the preceding Wednesday.
  it('Thursday 00:00 starts a new week (different from the preceding Wednesday)', () => {
    const thu = wacchoiWeekKey('2025/01/09(木) 00:00:00.00')
    const wed = wacchoiWeekKey('2025/01/08(水) 23:59:59.99')
    expect(thu).not.toBe(wed)
    expect(thu).toBe('2025/01/09')
    expect(wed).toBe('2025/01/02')
  })

  // Month boundary: 2025/01/01 (Wednesday) -> previous Thursday was 2024/12/26.
  it('month boundary: 2025/01/01(Wed) returns 2024/12/26', () => {
    expect(wacchoiWeekKey('2025/01/01(水) 00:00:00.00')).toBe('2024/12/26')
  })

  // Year boundary: 2025/01/02 (Thursday) -> itself.
  it('year boundary: 2025/01/02(Thu) returns 2025/01/02', () => {
    expect(wacchoiWeekKey('2025/01/02(木) 00:00:00.00')).toBe('2025/01/02')
  })

  // Parse failure cases -> null (safe side).
  it('returns null for null input', () => {
    expect(wacchoiWeekKey(null)).toBeNull()
  })

  it('returns null for empty string', () => {
    expect(wacchoiWeekKey('')).toBeNull()
  })

  it('returns null for a completely invalid date string', () => {
    expect(wacchoiWeekKey('not-a-date')).toBeNull()
  })

  it('returns null for a date with wrong format (missing leading slash)', () => {
    // Partial match would not trigger because leading digits are missing.
    expect(wacchoiWeekKey('2025-01-09')).toBeNull()
  })

  // Handles the format without day-of-week parentheses.
  it('handles format without day-of-week parentheses (2025/01/09 12:34:56)', () => {
    // 2025/01/09 is Thursday.
    expect(wacchoiWeekKey('2025/01/09 12:34:56')).toBe('2025/01/09')
  })
})
