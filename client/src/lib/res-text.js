// Display text of a res body.
//
// A body arrives as HTML the server already sanitized down to <a> and <br>. Two callers
// need the text a reader actually sees rather than the markup: the copy-body menu action
// and NG word matching. Both must agree, so the conversion lives here once.
//
// <br> becomes a newline first (textContent alone would silently drop line breaks), then
// textContent decodes the entities and strips the remaining tags. The browser owns the
// entity table, so no separate decoder is needed.
//
// The element is never attached to the document and only its textContent is read, so the
// parse has no rendering or script side effect — nothing here is inserted back into the
// page. (The body was already sanitized server-side; this is the same conversion the
// copy-body action used inline before it was shared.)
export function resBodyText(html) {
  const el = document.createElement('div')
  el.innerHTML = (html ?? '').replace(/<br\s*\/?>/gi, '\n')
  return el.textContent ?? ''
}
