import { describe, it, expect } from 'vitest'
import { linkify, extractImageUrls, normalizeImagePath, isImageUrl } from './linkify.js'

describe('linkify', () => {
  it('converts http URL to anchor', () => {
    const out = linkify('visit https://example.com here')
    expect(out).toContain('<a href="https://example.com"')
    expect(out).toContain('target="_blank"')
    expect(out).toContain('rel="noopener noreferrer"')
  })

  it('converts ttp URL to http anchor', () => {
    const out = linkify('ttp://example.com/x.html')
    expect(out).toContain('href="http://example.com/x.html"')
    // The link text still shows the original ttp:// form.
    expect(out).toContain('ttp://example.com/x.html')
  })

  it('converts ttps URL to https anchor', () => {
    const out = linkify('ttps://example.com/x.html')
    expect(out).toContain('href="https://example.com/x.html"')
  })

  it('converts >>N anchor to clickable span', () => {
    const out = linkify('&gt;&gt;123')
    expect(out).toContain('<span class="anchor" data-anchor="123">')
    expect(out).toContain('&gt;&gt;123</span>')
  })

  it('handles mixed anchors and URLs', () => {
    const out = linkify('&gt;&gt;5 see https://example.com for details')
    expect(out).toContain('data-anchor="5"')
    expect(out).toContain('href="https://example.com"')
  })

  it('does not double-wrap existing <a> tags', () => {
    const html = '<a href="https://example.com">link</a>'
    const out = linkify(html)
    // Should not produce nested <a> tags.
    const count = (out.match(/<a\b/g) || []).length
    expect(count).toBe(1)
  })

  it('does not wrap URL inside existing anchor href', () => {
    const html = '<a href="https://example.com/page">text https://example.com/page</a>'
    const out = linkify(html)
    // The href attribute URL must not be double-wrapped.
    const count = (out.match(/<a\b/g) || []).length
    expect(count).toBe(1)
  })
})

describe('extractImageUrls', () => {
  it('extracts jpg and png URLs', () => {
    const html = 'check https://img.example.com/photo.jpg and https://img.example.com/pic.png'
    const results = extractImageUrls(html)
    expect(results).toHaveLength(2)
    expect(results[0].path).toBe('img.example.com/photo.jpg')
    expect(results[1].path).toBe('img.example.com/pic.png')
  })

  it('deduplicates by normalized path', () => {
    // Both URLs normalize to the same path (query stripped).
    const html = 'https://img.example.com/a.jpg https://img.example.com/a.jpg'
    const results = extractImageUrls(html)
    expect(results).toHaveLength(1)
  })

  it('normalizes ttp URL href to http', () => {
    const html = 'ttp://img.example.com/pic.gif'
    const results = extractImageUrls(html)
    expect(results).toHaveLength(1)
    expect(results[0].href).toBe('http://img.example.com/pic.gif')
    expect(results[0].url).toBe('ttp://img.example.com/pic.gif')
  })

  it('stores original URL as url field for mosaic key', () => {
    const html = 'https://i.imgur.com/Abc123.jpg?size=large#x'
    const results = extractImageUrls(html)
    expect(results).toHaveLength(1)
    // The url field preserves the original (before path normalization removes query).
    expect(results[0].url).toContain('Abc123.jpg')
  })

  it('excludes non-image URLs', () => {
    const html = 'https://example.com/page.html https://img.com/a.png'
    const results = extractImageUrls(html)
    expect(results).toHaveLength(1)
    expect(results[0].path).toBe('img.com/a.png')
  })
})

describe('normalizeImagePath', () => {
  it('strips query and fragment', () => {
    expect(normalizeImagePath('https://i.imgur.com/Abc123.jpg?w=1#y')).toBe('i.imgur.com/Abc123.jpg')
  })

  it('lowercases host but not path', () => {
    const path = normalizeImagePath('https://IMG.EXAMPLE.COM/Path/Img.JPG')
    expect(path).toBe('img.example.com/Path/Img.JPG')
  })

  it('handles http scheme', () => {
    expect(normalizeImagePath('http://img.com/x.png')).toBe('img.com/x.png')
  })

  it('handles ttp scheme', () => {
    expect(normalizeImagePath('ttp://img.com/x.gif')).toBe('img.com/x.gif')
  })

  it('handles ttps scheme', () => {
    expect(normalizeImagePath('ttps://img.com/x.webp')).toBe('img.com/x.webp')
  })

  it('returns null for non-image URL', () => {
    expect(normalizeImagePath('https://example.com/page.html')).toBeNull()
  })

  it('returns null for unknown scheme', () => {
    expect(normalizeImagePath('ftp://img.com/x.png')).toBeNull()
  })
})

describe('isImageUrl', () => {
  it('returns true for jpg', () => {
    expect(isImageUrl('https://example.com/x.jpg')).toBe(true)
  })

  it('returns false for html', () => {
    expect(isImageUrl('https://example.com/page.html')).toBe(false)
  })
})
