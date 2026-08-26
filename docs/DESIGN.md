---
version: alpha
name: Sumi / 5ch-viewer
description: >
  5ch-viewer project overrides for the Sumi design system. The canonical
  template lives at ~/.claude/designs/sumi/DESIGN.md; this file records
  ONLY what is specific to 5ch-viewer (accent + functional data colors +
  domain components). CSS custom properties in client/src/App.svelte are
  the implementation of these tokens.
colors:
  # --- Project accent (amber) ---
  # Unsuffixed = Washi theme (light), -dark = Sumi theme (dark).
  # The Washi accent is darker than the template default (#9a6a00):
  # Washi is the e-paper theme, so the accent must read as ink and keep
  # white-on-accent well above 4.5:1 (here ~6.8:1) on the primary button.
  accent: "#7a5400"
  accent-subtle: "rgba(122, 84, 0, 0.12)"
  accent-dark: "#e0a800"
  accent-subtle-dark: "rgba(224, 168, 0, 0.15)"
  # --- Functional data colors (Washi / Sumi pairs) ---
  # Washi values form a darkness ramp (e-paper renders hue poorly, so
  # lightness carries the level; hue is a secondary cue). Sumi values
  # are the vivid originals and must never be changed from here.
  name: "#005500"
  name-dark: "#5bbf7a"
  star-on: "#8a6000"
  star-on-dark: "#e0a800"
  unread: "#7a3c00"
  unread-dark: "#ff9e1f"
  own: "#8f1c5a"
  own-dark: "#ff7ac0"
  badge-bg: "#a01818"
  badge-bg-dark: "#8c1f1f"
  badge-fg: "#ffffff"
  badge-fg-dark: "#ffffff"
  rate-0: "#9a9a9a"
  rate-1: "#1a8a9e"
  rate-2: "#1f7a33"
  rate-3: "#6e5a00"
  rate-4: "#7a3c00"
  rate-5: "#7a1414"
  rate-0-dark: "#555555"
  rate-1-dark: "#4dd6f0"
  rate-2-dark: "#57c46f"
  rate-3-dark: "#f0d020"
  rate-4-dark: "#ff9e1f"
  rate-5-dark: "#ff5a5a"
  id-l2: "#2a6fc0"
  id-l3: "#7a3db4"
  id-l4: "#8f1c5a"
  id-l5: "#7a1414"
  id-l2-dark: "#4d9ff0"
  id-l3-dark: "#a878f0"
  id-l4-dark: "#ff7ac0"
  id-l5-dark: "#ff5a5a"
---

# 5ch-viewer — Sumi Project Overrides

## Overview

**This project follows the Sumi design system.** The canonical template is
`~/.claude/designs/sumi/DESIGN.md` — all shared rules (neutral chrome,
typography scale, spacing/radius scales, iconography, focus ring,
component recipes) live there and are NOT restated here. This document
records only what is unique to 5ch-viewer. On chrome questions the
template wins; on the domain semantics below this file wins.

Accent: **amber** (`#7a5400` Washi / `#e0a800` Sumi). The Sumi value
equals the template default; the Washi value deliberately deviates from
the template default (`#9a6a00`) because Washi is the e-paper theme —
the darker amber reads as ink and gives the white-on-accent primary
button ~6.8:1 contrast.

## Colors

Everything below is a **functional data color** in the Sumi sense: it
encodes thread/post state, never decoration, and is exempt from the
one-accent rule. All come in Washi (light) / Sumi (dark) pairs and are
implemented as CSS custom properties in `client/src/App.svelte`
(`:root` = Washi, `[data-theme='dark']` = Sumi).

**Washi = e-paper.** All Washi data colors are dark, low-luminance inks:
level scales (rating, ID heat) are **lightness ramps** — monotonically
darker as the level rises — so the ordering survives grayscale rendering;
hue remains only as a secondary cue. The Sumi side keeps the vivid hues.

- **Name (#005500 / #5bbf7a):** Poster name and res number — the classic
  2ch green, kept as domain heritage. The Washi value is deepened for
  e-paper (~9:1 on white).
- **Star-on (#8a6000 / #e0a800):** Lit ★ glyphs in the rating picker.
  Deliberately decoupled from the chrome accent token; dark amber ink
  on Washi.
- **Rating scale rate-0..5:** 4px left color bar on favorite-list rows;
  groups list sections. 0 = 未分類 and is always the lightest, achromatic
  gray. Washi: gray → dark teal → dark green → olive → burnt orange →
  deep red, strictly darker at each step (relative luminance ~0.32 →
  0.21 → 0.14 → 0.11 → 0.07 → 0.05). Sumi: gray → cyan → green →
  yellow → orange → red.
- **Unread (#7a3c00 / #ff9e1f):** Orange 3px left border on unseen posts.
  Orange, not red — unread is not an error. On Washi it is burnt-orange
  ink (shares the rate-4 value, as the Sumi pair shares rate-4-dark).
- **Own (#8f1c5a / #ff7ac0):** Pink/magenta 3px left border on the user's
  own posts. **Takes priority over unread** when both would apply. Shares
  the id-l4 value in both themes.
- **Unread badge (badge-bg/badge-fg):** Dark-red pill with white bold count
  on list rows. Hidden when zero (not rendered). Already ink-dark
  (white-on-badge ~8:1), unchanged for e-paper. The count is
  **dat-backed**: `res_count` reflects only the locally saved dat (blob
  post count), never subject.txt alone — a visible count is a promise
  that tapping the row shows those posts instantly, with zero fetch.
  Rows whose dat has not yet been fetched (e.g. a freshly registered
  next thread) render no badge until the background sync (or a manual
  reload inside the opened thread) lands the dat.
- **ID heat id-l2..l5 (blue → purple → magenta → deep red):** ID badge
  color by post count of the same ID in a thread; l5 additionally bold.
  A single-occurrence ID (level 1) renders muted with no color. On Washi
  these are rendered as text, so every level keeps ≥4.5:1 on white while
  the lightness ramp (darker as the level rises) preserves the ordering
  in grayscale. Wacchoi badges inherit the same level color through the
  name span.

## Components

Domain components on top of the Sumi recipes:

- **Touch text-selection policy (app-wide):** on coarse-pointer touch
  devices — `@media (hover: none) and (pointer: coarse)` — the entire
  app is non-selectable, from **one** global rule on `body` in
  `client/src/App.svelte` (`user-select: none` +
  `-webkit-user-select: none` + `-webkit-touch-callout: none`). It
  covers everything by inheritance, including modals and the image
  viewer (both render inline in the app DOM; no portals). The **only
  exceptions are explicit text-entry fields** — `input`, `textarea`,
  and enabled `contenteditable` — which restore
  `user-select: text` / `-webkit-user-select: text` so typing, caret
  placement, range selection and copy/paste work normally. Fine-pointer
  (PC) environments are untouched: the media query never matches, so
  drag selection of res bodies and modal text stays default. Components
  must **not** re-declare touch `user-select` locally — the app-wide
  rule is the single owner. Custom long-press targets (res body, thread
  row) additionally keep an **unconditional element-level**
  `-webkit-touch-callout: none` (a desktop no-op) so the custom menu
  reliably beats the native callout regardless of inheritance quirks.
  Every copy path lost to the touch suppression must be compensated by
  an explicit menu copy action (本文をコピー, URL をコピー, ID コピー).
- **Thread row (favorites/archive list):** Sumi list row + 4px left bar in
  `rate-{n}`; dead threads at 50% opacity; unread pill trailing. The row
  is a long-press-menu target: it keeps the unconditional
  `-webkit-touch-callout: none` per the touch policy above.
- **Res card (thread view):** Sumi card; unread/own state shown by the 3px
  left border (see Colors). NG posts (NG ID / NG wacchoi / NG Word) replace
  the original header with a muted, struck-through disclosure in the form
  「N NG ID|NGワッチョイ|NG Word」. A post matching several rules shows exactly
  one reason, in that fixed order; removing it reveals the next. The body
  starts hidden and clicking the disclosure toggles it without restoring the
  original header.
  Right-click or a 500ms touch long-press anywhere on the NG card opens its
  dedicated one-action menu, 「[理由]から削除」. Removal is intentionally not
  offered from the original ID / wacchoi badge menus.
  **Res body context menu:** right-click (PC) or 500ms touch long-press
  on the body opens the reply menu — a standard Sumi context menu
  (modal, full-width default `.action` buttons) with 返信する,
  本文をコピー and NG Word に追加. The body keeps its unconditional element-level
  `-webkit-touch-callout: none`; touch selection suppression comes from
  the app-wide policy above. PC drag-selection is never suppressed —
  the body is Sumi's reading surface — and the menu's copy-body action
  compensates on touch. This applies uniformly to every res body
  rendering (main list, anchor tree, ID/wacchoi search modals).
- **Read-position dividers (thread view, newest-first model):** three hr-style
  ruled lines rendered around res cards in the main list only — the
  **thread end** (label 「おわり」) before the newest res, the
  **read boundary** (label 「前回ここまで」) immediately before the
  entry-time read-position res, and the **thread start** (label
  「はじまり」) after res 1 when it is distinct from the boundary.
  All share one recipe: a full-width flex row of two 1px hairlines in
  `border` with a centered caption-size (12px) `muted` label, 8px gap
  between line and label, 8px vertical margin (4px scale), transparent
  background. **They must read as a ruled line on the paper, never as a
  res card**: no `surface-raised` background, no border-radius, no card
  padding, no 3px left color bar, no data colors. They are static,
  non-interactive chrome (not a reading surface, not a button, no focus
  ring, no animation — Washi minimizes motion) and are **never**
  read-tracking targets (no `.res` class, no IntersectionObserver
  registration). Placement semantics: the boundary divider is frozen at
  the entry baseline — it does not move while reading (scroll advancing
  maxRead never moves it); a manual refresh may re-baseline it. Res cards
  are ordered newest to oldest. When the new section fills the viewport,
  the boundary's bottom edge is aligned with the scroll viewport bottom.
  When it is shorter, real older reses are rendered below the boundary in
  50-res batches until the viewport is naturally filled; synthetic blank
  spacer space must not be used. Remaining older reses are appended below
  in idle batches; this must not move the boundary on screen. With no previously read res (baseline 0), the
  boundary remains after res 1; otherwise it is immediately before the
  entry-time read-position res. When the boundary is at res 1, the thread
  start divider is omitted so duplicate dividers never share one position.
- **NG Word modal (thread view):** opened from the res body context menu.
  Column form in the post-modal rhythm (8px gap, 28rem, `.input` recipe):
  a **segmented control**, the pattern textarea prefilled with the res's
  display text, an 投稿者IDもNG checkbox (on by default, disabled when the
  post has no ID), an inline `.error` line, then キャンセル / 追加 as two
  equal-width buttons — 追加 is the modal's single primary (accent fill),
  matching the post modal's submit. The rule it writes is scoped to the
  board, never to the thread.
  **Segmented control:** two equal segments in one shared 1px `border`
  track with a 6px radius; the divider is the second segment's left border
  so no double hairline appears. The selected segment is filled with
  `accent-subtle` and takes `on-surface` text; unselected segments stay
  `surface-raised` with `muted` text. State is exposed as `aria-pressed`.
  It is chrome, so it never takes a data color, and it is used only where
  a choice has exactly two mutually exclusive values.
- **Anchor tree modal:** depth-indented nodes with a 2px border-left guide;
  the pivot res is highlighted with an accent left border.
- **ID / wacchoi badges:** caption-size, clickable (list modal), long-press
  or right-click opens the NG/copy/search menu. Color from the heat scale.
  Clickable affordance is `cursor: pointer` only; non-selectability comes
  from the app-wide touch policy.
- **Image thumbnails:** 96×96 cover crop, 4px radius; mosaic state = 20px
  blur (40px in the full-screen viewer). The viewer itself uses a
  near-black backdrop with white quiet controls (template-sanctioned
  exception to the token rule). The viewer overlay stays
  `user-select: none` unconditionally (both pointer types): it is a
  lightbox, not a reading surface, and its counter text is chrome.
- **Sticky footer actions (thread view only):** the thread view is the
  only screen with a sticky footer; **the favorites list has no footer
  and no refresh affordance** — it is display-only, kept fresh by the
  server-side background sync (`src/sync.rs`), so a manual bulk-refresh
  button would only add 5ch load. Footer actions are Sumi icon buttons
  (36×36, default variant, 18px monochrome SVG, always `aria-label`) —
  never text+icon composites, never circular FABs. **Refresh
  (`refresh-cw`) always sits at the right edge** — it is the most
  frequent action, so it gets the thumb-reach position; the refresh
  button disables while a refresh is in flight (disabled recipe: 50%
  opacity, no extra spinner required). The write action (pencil) sits
  at the left edge; the submit button inside the post modal is the
  screen's single primary (accent) button.
- **Zero-fetch entry (ChMate model):** entering a screen — the favorites
  list on mount **or** a thread — paints from SQLite immediately, with
  **zero fetch to 5ch**: no mount-time auto-refresh, no entry-time
  reload, no background warm-up, and therefore **no loading chrome on
  entry** (no spinner, no skeleton: the spinner recipe is reserved for
  real waits, and entry has none by design). Fetching new posts happens
  in exactly one UI place — the thread footer's refresh button (plus
  the automatic reload right after posting); the server-side background
  sync also fetches, but it is invisible to the UI and never adds
  chrome. The refresh-error notice (non-blocking danger text under the
  sticky title, saved posts still shown below) can therefore appear only
  after a failed manual refresh in the thread view — never on entry,
  and never on the favorites list (which has no fetch to fail).
- **Pull-to-refresh:** there is **no custom pull gesture anywhere** —
  a pull is delegated to the browser's native gesture and is simply a
  full page reload. Because entry (list and thread alike) renders saved
  data without touching 5ch, a native pull is **not an update gesture**:
  it re-renders saved data. Updating is exclusively the job of the
  thread footer's refresh button; the favorites list has no manual
  update path at all — the background sync keeps it fresh. The former
  quiet-panel recipe is retired. Consequently `overscroll-behavior`
  must **never** be set to `contain`/`none` on html/body or on internal
  scroll containers (e.g. the thread view's scrollable body): the
  scroll chain to the document has to stay open or the native gesture
  cannot fire.

## Do's and Don'ts

- Do keep the 2ch heritage colors (green names, dark-red unread pill) —
  they are domain identity, not chrome.
- Don't let unread and own borders stack; own wins.
- Do add any new post-state color here (with a dark pair) before using it
  in components.
- Don't brighten Washi data colors back toward the Sumi hues — Washi is
  the e-paper theme; darkness carries the level, hue is a secondary cue.
- Don't reintroduce custom pull-to-refresh UI or `overscroll-behavior`
  containment — native browser pull-to-refresh is the spec.
- Don't put any network wait between entry (favorites list mount or
  thread open) and first paint, and don't add loading chrome
  (spinner/skeleton) to entry — entry renders saved SQLite data
  instantly; the only refresh affordance is the thread footer's button.
- Don't reintroduce a manual bulk-refresh UI (footer, button, or gesture)
  on the favorites list — list freshness is the background sync's job;
  a manual path only adds 5ch load and violates the display-only list.
- Don't show an unread count that the saved dat cannot honor — the badge
  is dat-backed; subject.txt growth alone must never move it.
- Don't style the read-position dividers as cards or give them data
  colors — they are quiet ruled lines (`border` line, `muted` caption
  label) and must never be mistaken for a res.
- Don't suppress text selection on fine-pointer (PC) environments — the
  res body is the reading surface and modal text stays copyable there;
  suppression is the app-wide **touch-only** policy (see Components),
  always compensated by menu copy actions.
- Don't scatter per-component `user-select` rules for touch — the
  app-wide rule in `App.svelte` is the single owner. The only sanctioned
  element-level declarations are `user-select: text` on text-entry
  fields, the unconditional `-webkit-touch-callout: none` on custom
  long-press targets, and the image viewer's unconditional
  `user-select: none`.
