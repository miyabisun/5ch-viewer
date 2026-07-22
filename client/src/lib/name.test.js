import { describe, it, expect } from 'vitest'
import { formatName } from './name.js'

describe('formatName', () => {
  it('strips </b> <b> tags but keeps the wacchoi text', () => {
    const raw = 'iPhone774G </b>(ﾜｯﾁｮｲ b6f1-daVb [2001:268:77aa:c6e4:*])<b>'
    expect(formatName(raw)).toBe('iPhone774G (ﾜｯﾁｮｲ b6f1-daVb [2001:268:77aa:c6e4:*])')
  })

  it('leaves a plain name unchanged', () => {
    expect(formatName('名無しさん')).toBe('名無しさん')
  })

  it('strips arbitrary html tags', () => {
    expect(formatName('foo<i>bar</i>baz')).toBe('foobarbaz')
  })

  it('decodes basic entities', () => {
    expect(formatName('a &lt;b&gt; &amp; &quot;c&quot; &#39;d&#39;')).toBe('a <b> & "c" \'d\'')
  })

  it('does not double-decode &amp;lt;', () => {
    expect(formatName('&amp;lt;')).toBe('&lt;')
  })

  it('handles empty and nullish input', () => {
    expect(formatName('')).toBe('')
    expect(formatName(undefined)).toBe('')
    expect(formatName(null)).toBe('')
  })

  it('trims surrounding whitespace', () => {
    expect(formatName('  名無し  ')).toBe('名無し')
  })
})
