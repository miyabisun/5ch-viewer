import { describe, it, expect } from 'vitest'
import { scopedTo, matchesNgWord, findNgWord, isValidNgWord } from './ng.js'

const word = (server, board, kind, pattern) => ({ server, board, kind, pattern })

describe('scopedTo', () => {
  const rules = [
    word('egg', 'applism', 'text', 'A'),
    word('egg', 'other', 'text', 'B'),
    word('eagle', 'applism', 'text', 'C'),
  ]

  it('keeps only the rules of the given server+board', () => {
    expect(scopedTo(rules, 'egg', 'applism')).toEqual([rules[0]])
  })

  it('treats the same board name on another server as a different board', () => {
    expect(scopedTo(rules, 'eagle', 'applism')).toEqual([rules[2]])
  })

  it('returns an empty list for a board with no rules, and for no rules at all', () => {
    expect(scopedTo(rules, 'egg', 'nowhere')).toEqual([])
    expect(scopedTo(undefined, 'egg', 'applism')).toEqual([])
  })

  it('scopes NG ID rules by the same key', () => {
    const ids = [
      { server: 'egg', board: 'applism', ng_id: 'target' },
      { server: 'egg', board: 'other', ng_id: 'target' },
    ]
    expect(scopedTo(ids, 'egg', 'applism').map((r) => r.ng_id)).toEqual(['target'])
    expect(scopedTo(ids, 'egg', 'nowhere')).toEqual([])
  })
})

describe('matchesNgWord', () => {
  it('matches a literal substring anywhere in the text', () => {
    const rule = word('egg', 'applism', 'text', '荒らし')
    expect(matchesNgWord('これは荒らしの書き込みです', rule)).toBe(true)
    expect(matchesNgWord('荒らし', rule)).toBe(true)
    expect(matchesNgWord('ふつうの書き込み', rule)).toBe(false)
  })

  it('treats regex metacharacters literally for the text kind', () => {
    const rule = word('egg', 'applism', 'text', 'a.c')
    expect(matchesNgWord('xxa.cxx', rule)).toBe(true)
    expect(matchesNgWord('abc', rule)).toBe(false)
  })

  it('matches a regex pattern against the text', () => {
    const rule = word('egg', 'applism', 'regex', '^https?://\\S+$')
    expect(matchesNgWord('https://example.com/a', rule)).toBe(true)
    expect(matchesNgWord('前置き https://example.com/a', rule)).toBe(false)
  })

  it('matches across the newlines that separate body lines', () => {
    expect(matchesNgWord('1行目\n2行目', word('e', 'b', 'text', '目\n2'))).toBe(true)
    expect(matchesNgWord('1行目\n2行目', word('e', 'b', 'regex', '1行目\\n2行目'))).toBe(true)
  })

  it('never matches on empty text or an empty pattern', () => {
    expect(matchesNgWord('', word('e', 'b', 'text', 'x'))).toBe(false)
    expect(matchesNgWord('x', word('e', 'b', 'text', ''))).toBe(false)
  })

  it('does not match when the stored regex cannot compile', () => {
    expect(matchesNgWord('anything', word('e', 'b', 'regex', '('))).toBe(false)
  })
})

describe('findNgWord', () => {
  it('returns the matching rule so the caller can label and remove it', () => {
    const rules = [word('e', 'b', 'text', 'nope'), word('e', 'b', 'regex', '荒ら.')]
    expect(findNgWord('これは荒らしです', rules)).toEqual(rules[1])
  })

  it('returns null when nothing matches or there are no rules', () => {
    expect(findNgWord('本文', [word('e', 'b', 'text', 'x')])).toBeNull()
    expect(findNgWord('本文', [])).toBeNull()
  })
})

describe('isValidNgWord', () => {
  it('rejects an empty pattern for both kinds', () => {
    expect(isValidNgWord('text', '')).toBe(false)
    expect(isValidNgWord('regex', '')).toBe(false)
  })

  it('accepts any non-empty literal, including regex metacharacters', () => {
    expect(isValidNgWord('text', '(')).toBe(true)
  })

  it('accepts a compilable regex and rejects a broken one', () => {
    expect(isValidNgWord('regex', '^a(b|c)+$')).toBe(true)
    expect(isValidNgWord('regex', '(')).toBe(false)
    expect(isValidNgWord('regex', 'a{2,1}')).toBe(false)
  })
})
