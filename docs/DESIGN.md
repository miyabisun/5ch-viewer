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
  # --- Project accent (amber; happens to equal the template default) ---
  # Unsuffixed = Washi theme (light), -dark = Sumi theme (dark).
  accent: "#9a6a00"
  accent-subtle: "rgba(154, 106, 0, 0.12)"
  accent-dark: "#e0a800"
  accent-subtle-dark: "rgba(224, 168, 0, 0.15)"
  # --- Functional data colors (Washi / Sumi pairs) ---
  name: "#006600"
  name-dark: "#5bbf7a"
  star-on: "#e0a000"
  star-on-dark: "#e0a800"
  unread: "#ef8c00"
  unread-dark: "#ff9e1f"
  own: "#e84d9e"
  own-dark: "#ff7ac0"
  badge-bg: "#a01818"
  badge-bg-dark: "#8c1f1f"
  badge-fg: "#ffffff"
  badge-fg-dark: "#ffffff"
  rate-0: "#bbbbbb"
  rate-1: "#29b6d8"
  rate-2: "#3fae5a"
  rate-3: "#e0c000"
  rate-4: "#ef8c00"
  rate-5: "#e23b3b"
  rate-0-dark: "#555555"
  rate-1-dark: "#4dd6f0"
  rate-2-dark: "#57c46f"
  rate-3-dark: "#f0d020"
  rate-4-dark: "#ff9e1f"
  rate-5-dark: "#ff5a5a"
  id-l2: "#1a6fd8"
  id-l3: "#8a4fd8"
  id-l4: "#e84d9e"
  id-l5: "#e23b3b"
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

Accent: **amber** (`#9a6a00` Washi / `#e0a800` Sumi) — this is also the
template's default, so 5ch-viewer needs no accent override in practice.

## Colors

Everything below is a **functional data color** in the Sumi sense: it
encodes thread/post state, never decoration, and is exempt from the
one-accent rule. All come in Washi (light) / Sumi (dark) pairs and are
implemented as CSS custom properties in `client/src/App.svelte`
(`:root` = Washi, `[data-theme='dark']` = Sumi).

- **Name (#006600 / #5bbf7a):** Poster name and res number — the classic
  2ch green, kept as domain heritage.
- **Star-on (#e0a000 / #e0a800):** Lit ★ glyphs in the rating picker.
  Deliberately decoupled from the chrome accent token.
- **Rating scale rate-0..5 (gray → cyan → green → yellow → orange → red):**
  4px left color bar on favorite-list rows; groups list sections. 0 = 未分類.
- **Unread (#ef8c00 / #ff9e1f):** Orange 3px left border on unseen posts.
  Orange, not red — unread is not an error.
- **Own (#e84d9e / #ff7ac0):** Pink 3px left border on the user's own
  posts. **Takes priority over unread** when both would apply.
- **Unread badge (badge-bg/badge-fg):** Dark-red pill with white bold count
  on list rows. Hidden when zero (not rendered).
- **ID heat id-l2..l5 (blue → purple → pink → red):** ID badge color by
  post count of the same ID in a thread; l5 additionally bold. A
  single-occurrence ID (level 1) renders muted with no color. Wacchoi
  badges inherit the same level color through the name span.

## Components

Domain components on top of the Sumi recipes:

- **Thread row (favorites/archive list):** Sumi list row + 4px left bar in
  `rate-{n}`; dead threads at 50% opacity; unread pill trailing.
- **Res card (thread view):** Sumi card; unread/own state shown by the 3px
  left border (see Colors). NG posts (NG ID / NG wacchoi) render the
  header struck-through in muted with the body hidden entirely.
- **Anchor tree modal:** depth-indented nodes with a 2px border-left guide;
  the pivot res is highlighted with an accent left border.
- **ID / wacchoi badges:** caption-size, clickable (list modal), long-press
  or right-click opens the NG/copy/search menu. Color from the heat scale.
- **Image thumbnails:** 96×96 cover crop, 4px radius; mosaic state = 20px
  blur (40px in the full-screen viewer). The viewer itself uses a
  near-black backdrop with white quiet controls (template-sanctioned
  exception to the token rule).
- **Post FAB:** the write action is a Sumi icon button (pencil SVG) in the
  sticky thread footer; the submit button inside the post modal is the
  screen's single primary (accent) button.

## Do's and Don'ts

- Do keep the 2ch heritage colors (green names, dark-red unread pill) —
  they are domain identity, not chrome.
- Don't let unread and own borders stack; own wins.
- Do add any new post-state color here (with a dark pair) before using it
  in components.
