// Format a 5ch res name field for plain display.
//
// 5ch convention embeds the wacchoi / trip between </b> ... <b> markers, e.g.
//   "iPhone774G </b>(ﾜｯﾁｮｲ b6f1-daVb [...])<b>"
// The raw name leaks literal <b> / </b> tags into the UI. We strip ALL HTML tags
// (the wacchoi/trip TEXT between them is kept) and decode the basic entities that
// 5ch escapes, leaving a clean plain-text string. This is display-only and does
// not touch the post body (server-sanitized via ammonia).
export function formatName(name) {
  if (!name) return ''
  return decodeEntities(stripTags(name)).trim()
}

// Remove every HTML tag, keeping the text content between them.
function stripTags(s) {
  return s.replace(/<[^>]*>/g, '')
}

// Decode the small set of entities 5ch escapes in the name field.
function decodeEntities(s) {
  return s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&amp;/g, '&')
}
