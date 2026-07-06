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
  (white-on-badge ~8:1), unchanged for e-paper.
- **ID heat id-l2..l5 (blue → purple → magenta → deep red):** ID badge
  color by post count of the same ID in a thread; l5 additionally bold.
  A single-occurrence ID (level 1) renders muted with no color. On Washi
  these are rendered as text, so every level keeps ≥4.5:1 on white while
  the lightness ramp (darker as the level rises) preserves the ordering
  in grayscale. Wacchoi badges inherit the same level color through the
  name span.

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
- **Sticky footer actions:** footer actions are Sumi icon buttons
  (36×36, default variant, 18px monochrome SVG, always `aria-label`) —
  never text+icon composites, never circular FABs. **Refresh
  (`refresh-cw`) always sits at the right edge** — it is the most
  frequent action, so it gets the thumb-reach position; refresh buttons
  disable while a refresh is in flight (disabled recipe: 50% opacity,
  no extra spinner required). In the thread view the write action
  (pencil) sits at the left edge; the submit button inside the post
  modal is the screen's single primary (accent) button. Pull-to-refresh
  exists only at the **top** of the favorites list (shared quiet-panel
  recipe); there is no bottom pull gesture anywhere.

## Do's and Don'ts

- Do keep the 2ch heritage colors (green names, dark-red unread pill) —
  they are domain identity, not chrome.
- Don't let unread and own borders stack; own wins.
- Do add any new post-state color here (with a dark pair) before using it
  in components.
- Don't brighten Washi data colors back toward the Sumi hues — Washi is
  the e-paper theme; darkness carries the level, hue is a secondary cue.
