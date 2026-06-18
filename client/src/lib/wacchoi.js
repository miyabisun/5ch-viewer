// Wacchoi extraction and stats for 5ch res name fields.
//
// 5ch embeds a wacchoi token in the name column, between </b>…<b> markers:
//   "iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>"
// After formatName() strips the tags the text becomes:
//   "iPhone774G (ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])"
// These utilities extract the wacchoi token and build per-res stats used by
// ThreadView to colour-code same-wacchoi posts.

import { formatName } from './name.js'

// Escape HTML special characters to prevent XSS when inserting into {@html}.
// '&' must be replaced first to avoid double-escaping.
function escapeHtml(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

// Match the first [\w+]{4}-[\w+]{4} token found inside parentheses.
// The character class [\w+] includes '+' which appears in some wacchoi tokens.
// Lookbehind/lookahead (?<![\w+]) / (?![\w+]) replace \b so that:
//   - '+' (a non-word char) is accepted inside tokens, and
//   - longer tokens like (12345-67890) do not yield a false 4-4 sub-match.
const WACCHOI_RE = /\(.*?(?<![\w+])([\w+]{4}-[\w+]{4})(?![\w+]).*?\)/

// Extract the wacchoi key from a raw name string.
// Applies formatName first so HTML tags are stripped before matching.
// Returns the captured token (e.g. "7bb6-83IP"), or null if not found.
export function extractWacchoi(name) {
  if (!name) return null
  const text = formatName(name)
  const m = text.match(WACCHOI_RE)
  return m ? m[1] : null
}

// Return true when the thread's first res (num===1, or the array head if absent)
// contains a wacchoi token — meaning the whole thread has wacchoi enabled.
// This is the single source of truth for the enabled/disabled decision.
export function wacchoiEnabled(resList) {
  if (!resList || resList.length === 0) return false
  const first = resList.find((r) => r.num === 1) ?? resList[0]
  return extractWacchoi(first.name) != null
}

// colorLevel derived from total post count for the same wacchoi.
// Thresholds are identical to id.js so visual weight is consistent.
//   1     -> 'none'  (badge hidden)
//   2-3   -> 'l2'    (blue)
//   4-6   -> 'l3'    (purple)
//   7-9   -> 'l4'    (pink)
//   10+   -> 'l5'    (red)
function colorLevel(total) {
  if (total >= 10) return 'l5'
  if (total >= 7) return 'l4'
  if (total >= 4) return 'l3'
  if (total >= 2) return 'l2'
  return 'none'
}

// Convert the first wacchoi token in a name string into a clickable
// <span class="wacchoi-badge"> element, suitable for {@html} rendering.
//
// Processing order (XSS-safe):
//   1. formatName() strips HTML tags and decodes entities → clean plain text.
//   2. escapeHtml() re-escapes special chars so the result is safe for {@html}.
//   3. The first wacchoi token (same token extractWacchoi() finds) is wrapped in
//      a span.  Only [\w+]{4}-[\w+]{4} appears in data-wacchoi, so no new XSS vector.
//
// When no wacchoi token is present the escaped plain text is returned as-is.
// Empty/null/undefined input returns ''.
export function linkifyWacchoi(name) {
  if (!name) return ''
  // Match against the formatted text once (not via extractWacchoi, which would
  // re-run formatName) so the token and the rendered text stay in lockstep.
  const escaped = escapeHtml(formatName(name))
  const token = escaped.match(WACCHOI_RE)?.[1]
  if (!token) return escaped
  // Replace only the first occurrence of the token (String first-arg replace).
  return escaped.replace(
    token,
    `<span class="wacchoi-badge" data-wacchoi="${token}">${token}</span>`,
  )
}

// Extract only the suffix (zzzz, the 4 chars after the hyphen) from a raw name string
// OR directly from a wacchoi token string (e.g. '7bb6-83IP').
//
// When the input looks like a plain token (contains no parentheses), the suffix is taken
// directly (the part after the last hyphen) — this avoids re-running extractWacchoi on
// an already-extracted token, which would fail because extractWacchoi expects parentheses.
// When the input is a full name string, extractWacchoi is called first to pull the token.
// Returns null when no wacchoi token is found or the format is unexpected.
export function extractWacchoiSuffix(nameOrToken) {
  if (!nameOrToken) return null
  // Resolve to a bare token: full name strings (with parens) go through extractWacchoi;
  // values that already look like a token are used as-is. extractWacchoi only yields
  // [\w+]{4}-[\w+]{4}, so the suffix-length check below is always satisfied for that path.
  const token = nameOrToken.includes('(') ? extractWacchoi(nameOrToken) : nameOrToken
  if (!token) return null
  const idx = token.lastIndexOf('-')
  if (idx < 0) return null
  const suffix = token.slice(idx + 1)
  return suffix.length === 4 ? suffix : null
}

// Compute a Thursday-anchored week key from a 5ch date string.
//
// The wacchoi suffix (zzzz) resets every Thursday on 5ch. The exact reset time is
// unconfirmed; this implementation uses Thursday 00:00 JST as the boundary (provisional).
// See docs/5ch-spec.md §ワッチョイ for the rationale.
//
// Algorithm:
//   1. Parse the date string (e.g. '2026/06/15(月) 12:34:56.78') as a JST local date.
//      Using JST local date is appropriate because 5ch is a Japanese service and
//      both the server and typical viewers are in Japan.
//   2. Find the most recent Thursday on or before that date (day-of-week: Thursday=4).
//   3. Return that Thursday's date as 'YYYY/MM/DD'.
//
// Returns null on parse failure (safe side: NG not applied).
export function wacchoiWeekKey(dateStr) {
  if (!dateStr) return null
  // Extract YYYY/MM/DD from the 5ch date format.
  // Handles: '2026/06/15(月) 12:34:56.78', '2026/06/15 12:34:56', etc.
  const m = dateStr.match(/^(\d{4})\/(\d{2})\/(\d{2})/)
  if (!m) return null
  const year = parseInt(m[1], 10)
  const month = parseInt(m[2], 10) - 1 // Date month is 0-based
  const day = parseInt(m[3], 10)
  // Validate range to avoid Date overflow weirdness.
  if (isNaN(year) || isNaN(month) || isNaN(day)) return null

  const d = new Date(year, month, day)
  if (isNaN(d.getTime())) return null

  // Day of week: 0=Sun, 1=Mon, ..., 4=Thu, ..., 6=Sat.
  // Steps back to the most recent Thursday (same day if already Thursday).
  const dow = d.getDay()
  const daysBack = (dow - 4 + 7) % 7
  const thu = new Date(year, month, day - daysBack)

  const y = thu.getFullYear()
  const mo = String(thu.getMonth() + 1).padStart(2, '0')
  const da = String(thu.getDate()).padStart(2, '0')
  return `${y}/${mo}/${da}`
}

// Build a Map<resNum, { wacchoi, total, order, colorLevel }> from the res array.
// When enabled is false the function returns an empty Map immediately (no
// processing) — this is the primary guard for non-wacchoi threads.
// Only reses whose name contains a wacchoi token get an entry.
export function buildWacchoiStats(resList, enabled) {
  if (!enabled) return new Map()

  // Pass 1: assign each res its 1-based appearance order within its wacchoi and
  // accumulate the running count. total is patched to its final value in pass 2.
  const totals = new Map() // wacchoiKey -> final count
  const result = new Map() // resNum -> { wacchoi, total, order, colorLevel }
  for (const r of resList) {
    const wacchoi = extractWacchoi(r.name)
    if (!wacchoi) continue
    const order = (totals.get(wacchoi) ?? 0) + 1
    totals.set(wacchoi, order)
    result.set(r.num, { wacchoi, total: 0, order, colorLevel: 'none' })
  }

  // Pass 2: now that totals are known, fill in total + colorLevel per res.
  for (const stats of result.values()) {
    stats.total = totals.get(stats.wacchoi)
    stats.colorLevel = colorLevel(stats.total)
  }
  return result
}
