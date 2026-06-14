// ID extraction and stats for 5ch res date fields.
//
// The date field (dat column 3) looks like:
//   "2025/01/01(水) 12:34:56.78 ID:klSUPSuq0"
// These utilities extract the ID token, strip it for display, and build
// per-res stats (order / total / colorLevel) used by ThreadView.

const ID_RE = /\s*ID:([^\s]+)/

// Extract the ID key from a date string.
// Returns the part after "ID:" (e.g. "klSUPSuq0"), or null if not present.
// Only the first match is used (multiple IDs in one date field do not occur
// in practice, but defensive handling avoids surprises).
export function extractId(date) {
  if (!date) return null
  const m = date.match(ID_RE)
  return m ? m[1] : null
}

// Return the date string with the "ID:xxxx" token removed, trimmed.
// Used when the ID is either absent or shown separately as a coloured badge.
//
// Replacement strategy: absorb surrounding whitespace into a single space so
// that tokens on both sides of the ID stay separated (e.g. when a BE: token
// follows the ID). A trailing trim() removes any residual space if the ID was
// at the end of the string.
export function stripId(date) {
  if (!date) return ''
  return date.replace(/\s*ID:[^\s]+\s*/g, ' ').trim()
}

// colorLevel derived from total post count for the same ID.
// Returns a string key that maps 1:1 to a CSS class (id-l2 … id-l5).
//   1     -> 'none'  (ID hidden)
//   2-3   -> 'l2'    (blue)
//   4-6   -> 'l3'    (purple)
//   7-9   -> 'l4'    (pink)
//   10+   -> 'l5'    (red / "顔真っ赤")
function colorLevel(total) {
  if (total >= 10) return 'l5'
  if (total >= 7) return 'l4'
  if (total >= 4) return 'l3'
  if (total >= 2) return 'l2'
  return 'none'
}

// Build a Map<resNum, { id, total, order, colorLevel }> from the res array.
// Only reses whose date contains an ID token get an entry.
// Reses without an ID are omitted (caller treats absence as "no ID / default").
export function buildIdStats(resList) {
  // Pass 1: assign each res its 1-based appearance order within its ID and
  // accumulate the running total. total is patched to its final value in pass 2.
  const totals = new Map() // idKey -> final count
  const result = new Map() // resNum -> { id, total, order, colorLevel }
  for (const r of resList) {
    const id = extractId(r.date)
    if (!id) continue
    const order = (totals.get(id) ?? 0) + 1
    totals.set(id, order)
    result.set(r.num, { id, total: 0, order, colorLevel: 'none' })
  }

  // Pass 2: now that totals are known, fill in total + colorLevel per res.
  for (const stats of result.values()) {
    stats.total = totals.get(stats.id)
    stats.colorLevel = colorLevel(stats.total)
  }
  return result
}
