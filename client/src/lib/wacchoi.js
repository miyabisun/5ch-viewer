// Wacchoi extraction and stats for 5ch res name fields.
//
// 5ch embeds a wacchoi token in the name column, between </b>…<b> markers:
//   "iPhone774G </b>(ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])<b>"
// After formatName() strips the tags the text becomes:
//   "iPhone774G (ﾜｯﾁｮｲ 7bb6-83IP [2400:4050:c4e1:e900:*])"
// These utilities extract the wacchoi token and build per-res stats used by
// ThreadView to colour-code same-wacchoi posts.

import { formatName } from './name.js'

// Match the first \w{4}-\w{4} token found inside parentheses.
// Word-boundary anchors (\b) prevent partial matches against longer hex strings.
const WACCHOI_RE = /\(.*?\b(\w{4}-\w{4})\b.*?\)/

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

// Remove the parenthesised wacchoi group from a formatted name, leaving the
// rest of the name intact. Returns the cleaned, trimmed name.
// When no wacchoi is present, returns formatName(name) unchanged (= legacy behaviour).
export function stripWacchoi(name) {
  const text = formatName(name)
  if (!extractWacchoi(name)) return text
  return text.replace(WACCHOI_RE, '').replace(/\s{2,}/g, ' ').trim()
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
