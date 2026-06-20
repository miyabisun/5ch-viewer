// linkify.js — URL linkification and image URL extraction for post bodies.
//
// Server-sanitized body text is passed in as HTML; this module further
// processes it to:
//   1. Convert plain-text http/https/ttp/ttps URLs into <a> elements.
//   2. Convert >>N anchors into clickable spans.
//   3. Extract image URLs for thumbnail display.
//
// The server strips thread-internal anchors and normalizes ttp→http in
// href attributes; here we handle bare URLs in body text that are not
// already wrapped in <a>.

// Matches >>N anchors (already-sanitized: >> appears as &gt;&gt;).
export const ANCHOR_RE = /(?:&gt;){2}(\d+)/g

// Matches bare http/https/ttp/ttps image URLs ending with a recognized extension.
// Must match the same set as the Rust regex in goch::images.
const IMAGE_URL_RE = /\b(h?ttps?:\/\/[^\s<>"']+?\.(?:png|jpe?g|gif|webp))\b/gi

// Matches any bare http/https/ttp/ttps URL (non-image links included).
// Used to linkify all external URLs found in body text.
const URL_RE = /\b(h?ttps?:\/\/[^\s<>"']+)/gi

const SCHEMES = ['https://', 'http://', 'ttps://', 'ttp://']
const IMAGE_EXTS = ['.png', '.jpg', '.jpeg', '.gif', '.webp']

/**
 * Normalizes an image URL into a cache path (mirrors Rust's normalize_image_path).
 * Returns null when the URL is not a recognized image URL.
 */
export function normalizeImagePath(url) {
  const scheme = SCHEMES.find((s) => url.startsWith(s))
  if (!scheme) return null

  // Strip query string, fragment, and trailing slash.
  const path = url.slice(scheme.length).split(/[?#]/)[0].replace(/\/$/, '')

  // Must end with a recognized image extension.
  const lower = path.toLowerCase()
  if (!IMAGE_EXTS.some((ext) => lower.endsWith(ext))) return null

  // Lowercase the host part only (everything before the first '/').
  const slashIdx = path.indexOf('/')
  if (slashIdx === -1) return path.toLowerCase()
  return path.slice(0, slashIdx).toLowerCase() + path.slice(slashIdx)
}

/** Returns true when the URL ends with a recognized image extension. */
export function isImageUrl(url) {
  return normalizeImagePath(url) !== null
}

/**
 * Normalizes a ttp/ttps URL to http/https for use in href attributes.
 * Returns the URL unchanged when it already starts with http/https.
 */
function normalizeScheme(url) {
  if (url.startsWith('ttps://')) return 'https://' + url.slice('ttps://'.length)
  if (url.startsWith('ttp://')) return 'http://' + url.slice('ttp://'.length)
  return url
}

/**
 * linkify(html): converts bare URLs in sanitized body HTML into <a> elements
 * and >>N anchors into clickable spans.
 *
 * Already-linked URLs (inside existing <a> tags) are NOT double-wrapped.
 * ttp/ttps schemes are normalized to http/https in the href.
 */
export function linkify(html) {
  // Step 1: convert >>N anchors.
  let result = html.replace(ANCHOR_RE, '<span class="anchor" data-anchor="$1">&gt;&gt;$1</span>')

  // Step 2: convert bare URLs that are not already inside an <a> tag.
  // We process the HTML as a string, tracking whether we are inside an existing
  // <a> element to avoid double-wrapping.
  result = linkifyUrls(result)

  return result
}

/**
 * Linkifies bare URLs in HTML text, skipping content inside existing <a> tags.
 */
function linkifyUrls(html) {
  // Split on existing <a ...>...</a> blocks to avoid double-wrapping.
  // We process text segments only, leaving tag segments intact.
  const parts = splitOnAnchors(html)
  return parts
    .map((part) => {
      if (part.isAnchor) return part.text
      return part.text.replace(URL_RE, (match) => {
        const href = normalizeScheme(match)
        return `<a href="${href}" target="_blank" rel="noopener noreferrer">${match}</a>`
      })
    })
    .join('')
}

/**
 * Splits an HTML string into segments: text segments and existing anchor blocks.
 * Returns an array of { text, isAnchor } objects.
 */
function splitOnAnchors(html) {
  const segments = []
  // Match full <a ...>...</a> blocks (non-greedy inner content).
  const anchorBlockRe = /<a\b[^>]*>[\s\S]*?<\/a>/gi
  let last = 0
  let m
  while ((m = anchorBlockRe.exec(html)) !== null) {
    if (m.index > last) {
      segments.push({ text: html.slice(last, m.index), isAnchor: false })
    }
    segments.push({ text: m[0], isAnchor: true })
    last = m.index + m[0].length
  }
  if (last < html.length) {
    segments.push({ text: html.slice(last), isAnchor: false })
  }
  return segments
}

/**
 * extractImageUrls(html): extracts all image URLs from sanitized body HTML.
 * Returns an array of { href, path, url } objects (deduplicated by path).
 *   - url  : the original URL string (used as the mosaic lookup key)
 *   - href : the effective URL for the <img src> (http/https normalized)
 *   - path : the normalized cache path for /api/images/{*path}
 */
export function extractImageUrls(html) {
  // Strip existing <a> tag attributes to avoid extracting URLs from href values
  // (those are already linkified by the server).
  // We scan the plain text content of the HTML body.
  const seenPaths = new Set()
  const results = []

  // Reset lastIndex before use (global regex is stateful).
  IMAGE_URL_RE.lastIndex = 0
  let m
  while ((m = IMAGE_URL_RE.exec(html)) !== null) {
    const url = m[1]
    const path = normalizeImagePath(url)
    if (!path || seenPaths.has(path)) continue
    seenPaths.add(path)
    const href = normalizeScheme(url)
    results.push({ href, path, url })
  }

  return results
}

// Re-export ANCHOR_RE so ThreadView can reuse it for backrefs without a separate import.
export { ANCHOR_RE as default }
