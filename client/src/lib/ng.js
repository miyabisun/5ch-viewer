// NG rule scoping and matching.
//
// Both NG IDs and NG words are stored per (server, board) and never per thread, so a
// rule hides matching posts in every thread of that board and in no other board. The
// server returns the full rule list; these helpers narrow it to the board being viewed
// and decide whether a post matches.
//
// NG word kinds:
//   'text'  — literal substring of the post's display text
//   'regex' — JavaScript regular expression tested against the display text
//
// The browser's RegExp is the only engine that ever evaluates a stored pattern, so it
// is also the only validator: isValidRegex() below is what decides whether a pattern
// may be saved. The Rust side deliberately does not re-check regex syntax.

// Narrow a rule list to the board being viewed.
export function scopedTo(rules, server, board) {
  return (rules ?? []).filter((r) => r.server === server && r.board === board)
}

// True when the display text matches the rule. An invalid regex never matches (a
// pattern is validated before it is saved, so this only guards rules that predate a
// browser-engine difference).
export function matchesNgWord(text, rule) {
  if (!text || !rule?.pattern) return false
  if (rule.kind === 'regex') {
    try {
      return new RegExp(rule.pattern).test(text)
    } catch {
      return false
    }
  }
  return text.includes(rule.pattern)
}

// Return the first rule in the list that matches the display text, or null.
// The caller uses the returned rule both to label the hidden post and to remove it.
export function findNgWord(text, rules) {
  return (rules ?? []).find((rule) => matchesNgWord(text, rule)) ?? null
}

// True when the pattern can be saved: non-empty, and (for 'regex') compilable by the
// engine that will evaluate it.
export function isValidNgWord(kind, pattern) {
  if (!pattern) return false
  if (kind !== 'regex') return true
  try {
    new RegExp(pattern)
    return true
  } catch {
    return false
  }
}
