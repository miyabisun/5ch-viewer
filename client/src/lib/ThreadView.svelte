<script>
  import { untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'
  import { formatName } from './name.js'
  import { stripId, buildIdStats } from './id.js'
  import { buildWacchoiStats, wacchoiEnabled, linkifyWacchoi, extractWacchoiSuffix, wacchoiWeekKey } from './wacchoi.js'
  import { copyText } from './clipboard.js'
  import Modal from './Modal.svelte'
  import { pullRefresh, PULL_THRESHOLD_PX } from './pullRefresh.js'

  let { fav, onback, onprogress = () => {}, ngIds = new Set(), onngchange = () => {}, ngWacchoi = [], onngwacchoichange = () => {} } = $props()

  let data = $state(null)
  // Surfaced when the auto-refresh (reload) fails. The stored dat is still shown
  // below, so this is a non-blocking notice rather than a hard error.
  let refreshError = $state(null)
  // Pull-to-refresh: true while a manual refresh triggered by the gesture is in flight.
  // Used to prevent double-firing and to show the loading panel.
  let refreshing = $state(false)
  // Current pull-panel offset in px (0 = hidden). Updated by the pullRefresh action callback.
  let pullPx = $state(0)
  // Whether the panel shows 'dragging' hint (above threshold = 'release to refresh').
  let pullPhase = $state('idle') // 'idle' | 'dragging'
  // Read position (max res number that has passed through the viewport). Initialized from the saved read_res (only on first mount).
  let maxRead = $state(untrack(() => fav.read_res))
  // Unread-bar baseline: reses with num > readBaseline show the unread left-border.
  // Initialized to fav.read_res like maxRead. Raised on pull-to-refresh so that
  // pre-refresh reses lose their bar and only newly-added reses stay unread.
  let readBaseline = $state(untrack(() => fav.read_res))
  // Restore-on-open guard: scroll to the saved read position only after the first
  // successful load, never after a manual reload.
  let restored = $state(false)

  // Open = auto-refresh. Fetch the latest (GET reload: checks subject.txt's
  // res_count and pulls new dat if it grew), then render the dat. The restore
  // effect runs after this single load, so there is no double-fetch and the
  // read-position restore happens against the up-to-date list (new posts land
  // naturally below the restored position).
  async function load() {
    try {
      await api.reload(fav.server, fav.board, fav.thread_id)
      refreshError = null
    } catch (e) {
      // Refresh is best-effort for display (the stored dat is still shown below),
      // but the failure must not be silently swallowed: surface it to the user and
      // the console so a "stuck on old posts" situation is diagnosable.
      refreshError = e.message
      console.error('[reload]', e)
    }
    data = await api.getDat(fav.server, fav.board, fav.thread_id)
    if (data.read_res > maxRead) maxRead = data.read_res
  }

  // Manual refresh triggered by the pull-to-refresh gesture.
  // Guards against double-fire via the `refreshing` flag.
  // Before loading, advance readBaseline (and maxRead) to the last visible res
  // number so that all currently-shown reses lose their unread bar and only
  // newly-added reses (num > old last) will show it after the fetch.
  async function triggerRefresh() {
    if (refreshing) return
    refreshing = true
    // Advance baseline to the last res currently in data (user reached the bottom
    // to trigger this gesture, so those reses are considered read).
    const lastNum = data?.res?.length ? data.res[data.res.length - 1].num : maxRead
    readBaseline = Math.max(readBaseline, lastNum)
    maxRead = Math.max(maxRead, lastNum)
    try {
      await load()
    } finally {
      refreshing = false
    }
  }

  // Returns true when any modal/overlay is open. Used by both backSwipe and pullRefresh
  // to suppress gestures while overlays are in front.
  function isAnyModalOpen() {
    return (
      anchorRoot != null ||
      idListId != null ||
      idMenu != null ||
      wacchoiListKey != null ||
      wacchoiMenu != null ||
      idSearchLoading ||
      idSearchResult != null ||
      wacchoiSearchLoading ||
      wacchoiSearchResult != null
    )
  }

  // Initial load (auto-refresh on open).
  $effect(() => {
    load()
  })

  // Restore the saved read position by scrolling the last-read res into view.
  // Runs once after the dat has rendered (in an effect, not load, so the res
  // nodes exist in the DOM); the `restored` guard prevents re-runs.
  $effect(() => {
    if (!data || restored) return
    restored = true
    const target = Math.max(data.read_res, fav.read_res)
    if (target < 1) return
    // Defer to after the list renders, then scroll the saved res into view.
    requestAnimationFrame(() => {
      const node = document.querySelector(`.res[data-res="${target}"]`)
      if (node) {
        // scrollIntoView automatically targets the nearest scrollable ancestor
        // (detail-pane on PC, window on phone) — no manual branching needed.
        node.scrollIntoView({ block: 'end' })
        return
      }
      // Fallback (target res not rendered): scroll to the bottom of whichever
      // element actually scrolls. On PC (>=768px) the detail-pane is the scroll
      // container; on phone it is window. matchMedia is the reliable signal —
      // on phone the detail-pane is display:block, so offsetParent is non-null
      // and cannot be used to tell the two apart.
      const pane = window.matchMedia('(min-width: 768px)').matches
        ? document.querySelector('.detail-pane')
        : null
      if (pane) pane.scrollTop = pane.scrollHeight
      else window.scrollTo(0, document.body.scrollHeight)
    })
  })

  // Track visible reses with IntersectionObserver and update maxRead.
  // NOTE: Scaffold implementation. The case where the observer is not yet
  //       created when the action runs does not occur on list re-render,
  //       but stricter handling needs consideration (see docs).
  let observer
  $effect(() => {
    observer = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          const n = Number(e.target.dataset.res)
          if (n > maxRead) maxRead = n
        }
      }
    })
    return () => observer?.disconnect()
  })

  function track(node, num) {
    node.dataset.res = num
    let obs = observer
    obs?.observe(node)
    return {
      destroy() {
        obs?.unobserve(node)
      },
    }
  }

  // Debounced send (2s after scrolling stops).
  // onprogress fires immediately (no debounce) so the list pane updates in real time.
  let timer
  $effect(() => {
    const n = maxRead
    // Notify the parent immediately so the list unread badge reflects the new position.
    onprogress(n)
    clearTimeout(timer)
    timer = setTimeout(() => {
      api.setProgress(fav.server, fav.board, fav.thread_id, n).catch(() => {})
    }, 2000)
    return () => clearTimeout(timer)
  })

  // On unload, reliably send the final position via sendBeacon.
  $effect(() => {
    const sendBeacon = () =>
      beaconProgress(fav.server, fav.board, fav.thread_id, maxRead)
    const onHide = () => {
      if (document.visibilityState === 'hidden') sendBeacon()
    }
    const onPageHide = sendBeacon
    document.addEventListener('visibilitychange', onHide)
    window.addEventListener('pagehide', onPageHide)
    return () => {
      document.removeEventListener('visibilitychange', onHide)
      window.removeEventListener('pagehide', onPageHide)
    }
  })

  // In-body anchor >>N. The body is already sanitized, so >> appears as &gt;&gt;.
  const ANCHOR_RE = /(?:&gt;){2}(\d+)/g

  // Convert anchors (>>123) into clickable spans (the body is already sanitized on the server).
  // data-anchor only contains digits, so no new XSS vector is introduced.
  function linkify(html) {
    return html.replace(
      ANCHOR_RE,
      '<span class="anchor" data-anchor="$1">&gt;&gt;$1</span>',
    )
  }

  // Back-reference map: N -> [res numbers that anchor to N...].
  // Built on the front-end by parsing >>N in each res body (no server change needed).
  const backrefs = $derived.by(() => {
    const map = new Map()
    if (!data?.res) return map
    for (const r of data.res) {
      const seen = new Set()
      let m
      while ((m = ANCHOR_RE.exec(r.body)) !== null) {
        const target = Number(m[1])
        if (target === r.num || seen.has(target)) continue
        seen.add(target)
        if (!map.has(target)) map.set(target, [])
        map.get(target).push(r.num)
      }
    }
    return map
  })

  // Per-res ID stats: Map<resNum, { id, total, order, colorLevel }>.
  // Built from all reses in one pass; used by resHead to colour-code same-ID posts.
  const idStats = $derived.by(() => buildIdStats(data?.res ?? []))

  // Wacchoi support: enabled only for threads whose first res contains a wacchoi token.
  const wacchoiEnabledFlag = $derived(wacchoiEnabled(data?.res ?? []))
  // Per-res wacchoi stats: Map<resNum, { wacchoi, total, order, colorLevel }>.
  // Empty Map when the thread has no wacchoi (wacchoiEnabledFlag=false).
  const wacchoiStats = $derived.by(() =>
    buildWacchoiStats(data?.res ?? [], wacchoiEnabledFlag),
  )

  // Resolve a res's ID: server-extracted r.id is authoritative; fall back to
  // client extraction (idStats) for edge cases such as id-search result reses.
  function resolveId(r) {
    return r.id ?? idStats.get(r.num)?.id ?? null
  }

  // Resolve a res's wacchoi from the client-side wacchoiStats map.
  // No server-side field exists for wacchoi, so we always use the extracted value.
  function resolveWacchoi(r) {
    return wacchoiStats.get(r.num)?.wacchoi ?? null
  }

  // Look up a res by number (missing if not found).
  function resOf(num) {
    return data?.res.find((r) => r.num === num) ?? { num, missing: true }
  }

  // Root anchor number; null means the modal is closed.
  let anchorRoot = $state(null)

  function openAnchor(num) {
    anchorRoot = num
  }
  function closeAnchor() {
    anchorRoot = null
  }

  // Build the anchor tree for the current anchorRoot (N) as a flat display list.
  // Each element: { num, depth, highlight }
  //   - depth:     logical generation depth (root ancestor = 0, each reply = +1)
  //   - highlight: true only for anchorRoot (the clicked res)
  //
  // Forward-anchor chain (parents) is walked upward from anchorRoot; the deepest
  // ancestor gets depth 0.  Backref chain (children) is walked downward from
  // anchorRoot; each level adds 1 to anchorRoot's depth.
  //
  // A single shared `visited` set (seeded with anchorRoot) spans both walks, so it
  // prevents cycles, DAG duplication, and ancestors reappearing as descendants.
  const anchorTree = $derived.by(() => {
    if (anchorRoot == null || !data?.res) return null

    // Forward anchors of `num`: the reses it references via >>M.
    // matchAll clones the regex internally, so ANCHOR_RE.lastIndex is not corrupted.
    const forwardAnchors = (num) => {
      const r = resOf(num)
      if (r.missing) return []
      return [...r.body.matchAll(ANCHOR_RE)]
        .map((m) => Number(m[1]))
        .filter((target) => target !== num)
    }
    // Backrefs of `num`: the reses that reference it.
    const backAnchors = (num) => backrefs.get(num) ?? []

    // Shared DFS for both directions. Follows `neighbours(num)`, marks each node in
    // the shared `visited` set (cycle-safe + DAG-dedupe), and invokes `visit(num, dist)`
    // for every newly-reached node (dist = edges from start). The visited set is shared
    // across both walks so ancestors never reappear as descendants.
    function walk(start, neighbours, visited, visit, dist = 1) {
      for (const next of neighbours(start)) {
        if (visited.has(next)) continue
        visited.add(next)
        visit(next, dist)
        walk(next, neighbours, visited, visit, dist + 1)
      }
    }

    // Walk upward, keeping the longest distance per ancestor (reached via multiple
    // paths) so it renders as shallow as possible.
    const visited = new Set([anchorRoot])
    const distMap = new Map() // num -> max distance from anchorRoot
    walk(anchorRoot, forwardAnchors, visited, (num, dist) =>
      distMap.set(num, Math.max(distMap.get(num) ?? 0, dist)),
    )

    // self.depth = length of the longest ancestor chain.
    const selfDepth = distMap.size > 0 ? Math.max(...distMap.values()) : 0

    // Ancestors: depth = selfDepth - distance; sorted shallowest-first (oldest at top).
    const ancestors = [...distMap.entries()]
      .map(([num, dist]) => ({ num, depth: selfDepth - dist }))
      .sort((a, b) => a.depth - b.depth)

    // Descendants: each backref level adds 1 to selfDepth; pushed in DFS order.
    const descendants = []
    walk(anchorRoot, backAnchors, visited, (num, dist) =>
      descendants.push({ num, depth: selfDepth + dist }),
    )

    return [
      ...ancestors,
      { num: anchorRoot, depth: selfDepth, highlight: true },
      ...descendants,
    ]
  })

  // Body click (shared by list and modal). Follow the anchor or open wacchoi list when tapped.
  function onBodyClick(e) {
    const w = e.target.closest('.wacchoi-badge')
    if (w) {
      // If a long-press already opened the wacchoi menu, swallow this click to avoid
      // opening the list modal on top of the menu.
      if (wacchoiLongPressed) {
        wacchoiLongPressed = false
        e.stopPropagation()
        return
      }
      openWacchoiList(w.dataset.wacchoi)
      return
    }
    const a = e.target.closest('.anchor')
    if (!a) return
    openAnchor(Number(a.dataset.anchor))
  }

  // Touch action: a clear right-swipe goes back to the list.
  // Thresholds (mirrors novel-server's reader swipe to avoid misfires):
  //   - lock the gesture as horizontal only when |dx| > |dy| after 5px of travel,
  //   - require |dx| >= 60px and a horizontal dominance (|dx| > |dy| * 1.5) on end.
  //
  // The swipe must never be confused with an anchor tap (regression: tapping >>N
  // opened the modal but then navigated back to the list). Guards:
  //   1. A touch that *starts* on an anchor is an anchor interaction, not a swipe
  //      candidate -> never trigger back for it.
  //   2. While a modal is open, the gesture is for the modal, not for leaving the
  //      thread -> never trigger back.
  //   3. Multi-touch (pinch/zoom) is ignored.
  function backSwipe(node) {
    let startX, startY, locked, horizontal, ignore
    function onStart(e) {
      // Ignore multi-touch and touches that begin on an interactive anchor or wacchoi badge.
      ignore =
        e.touches.length > 1 ||
        !!e.target.closest('.anchor') ||
        !!e.target.closest('.wacchoi-badge')
      const t = e.touches[0]
      startX = t.clientX
      startY = t.clientY
      locked = false
      horizontal = false
    }
    function onMove(e) {
      if (ignore) return
      const t = e.touches[0]
      const dx = t.clientX - startX
      const dy = t.clientY - startY
      if (!locked) {
        if (Math.abs(dx) < 5 && Math.abs(dy) < 5) return
        locked = true
        horizontal = Math.abs(dx) > Math.abs(dy)
      }
    }
    function onEnd(e) {
      if (ignore || !locked || !horizontal) return
      // Any modal open: the gesture belongs to the modal, not the thread.
      if (isAnyModalOpen()) return
      const dx = e.changedTouches[0].clientX - startX
      const dy = e.changedTouches[0].clientY - startY
      if (dx >= 60 && Math.abs(dx) > Math.abs(dy) * 1.5) onback()
    }
    node.addEventListener('touchstart', onStart, { passive: true })
    node.addEventListener('touchmove', onMove, { passive: true })
    node.addEventListener('touchend', onEnd, { passive: true })
    return {
      destroy() {
        node.removeEventListener('touchstart', onStart)
        node.removeEventListener('touchmove', onMove)
        node.removeEventListener('touchend', onEnd)
      },
    }
  }

  // --- ID left-click list modal ---
  // Shows all reses in the current thread posted by the same ID.
  let idListId = $state(null)

  function openIdList(id) {
    idListId = id
  }
  function closeIdList() {
    idListId = null
  }

  // Reses for the ID-list modal, derived from in-memory data (no extra fetch).
  const idListRes = $derived.by(() => {
    if (idListId == null || !data?.res) return []
    return data.res.filter((r) => resolveId(r) === idListId)
  })

  // --- Wacchoi left-click list modal ---
  // Shows all reses in the current thread posted under the same wacchoi.
  let wacchoiListKey = $state(null)

  function openWacchoiList(wacchoi) {
    wacchoiListKey = wacchoi
  }
  function closeWacchoiList() {
    wacchoiListKey = null
  }

  // Reses for the wacchoi-list modal, derived from in-memory data (no extra fetch).
  const wacchoiListRes = $derived.by(() => {
    if (wacchoiListKey == null || !data?.res) return []
    return data.res.filter((r) => resolveWacchoi(r) === wacchoiListKey)
  })

  // --- Wacchoi right-click / long-press menu ---
  // Menu state: { wacchoi, date } for the active menu, or null (closed).
  // Storing the res date at open time lets toggleNgWacchoi compute the week_key
  // without needing to re-traverse the DOM.
  let wacchoiMenu = $state(null)

  function openWacchoiMenu(wacchoi, date) {
    wacchoiMenu = { wacchoi, date }
  }
  function closeWacchoiMenu() {
    wacchoiMenu = null
  }

  // Resolve the date string from the event target.
  // The date is stored in the closest .name[data-date] ancestor span, which is
  // always present in every render context (main list, anchor tree, id-list modal,
  // wacchoi-list modal). This avoids depending on .res[data-res] which is only
  // set on main-list divs via use:track.
  function resDateFromTarget(target) {
    return target.closest('.name[data-date]')?.dataset.date ?? null
  }

  // Right-click on the .name span: open the menu if the target is a wacchoi badge.
  // Delegated here (rather than on the badge) because the badge is rendered via
  // {@html} inside .name and has no own Svelte event bindings.
  function onWacchoiContextMenu(e) {
    const badge = e.target.closest('.wacchoi-badge')
    if (!badge) return
    e.preventDefault()
    const date = resDateFromTarget(badge)
    openWacchoiMenu(badge.dataset.wacchoi, date)
  }

  // Long-press detection for the wacchoi badge (touch devices, 500ms, same pattern as ID).
  let wacchoiPressTimer
  let wacchoiLongPressed = false
  function onWacchoiPointerDown(e) {
    if (e.pointerType !== 'touch') return
    const badge = e.target.closest('.wacchoi-badge')
    if (!badge) return
    wacchoiLongPressed = false
    wacchoiPressTimer = setTimeout(() => {
      wacchoiLongPressed = true
      const date = resDateFromTarget(badge)
      openWacchoiMenu(badge.dataset.wacchoi, date)
    }, 500)
  }
  function cancelWacchoiPress() {
    clearTimeout(wacchoiPressTimer)
  }

  // Copy the wacchoi string (with the "ワッチョイ " prefix) to the clipboard.
  async function copyWacchoi(w) {
    await copyText('ワッチョイ ' + w)
    closeWacchoiMenu()
  }

  // True when the given wacchoi token, posted on the given date, is in the NG list
  // for the current board + Thursday-anchored week. The single source of truth for
  // the board+week-scoped match, shared by the menu state and the per-res NG filter.
  // Returns false (safe side) when the suffix/week/wacchoi/date are unresolvable.
  function isWacchoiNgFor(wacchoi, date) {
    if (!wacchoi || !date) return false
    const suffix = extractWacchoiSuffix(wacchoi)
    const weekKey = wacchoiWeekKey(date)
    if (!suffix || !weekKey) return false
    return ngWacchoi.some(
      (e) => e.suffix === suffix && e.board === fav.board && e.week_key === weekKey,
    )
  }

  // Add or remove the wacchoi from the NG list. Calls onngwacchoichange to let the parent reload.
  async function toggleNgWacchoi(wacchoi, date) {
    const suffix = extractWacchoiSuffix(wacchoi)
    const weekKey = wacchoiWeekKey(date)
    if (!suffix || !weekKey) {
      console.error('[ng-wacchoi] cannot compute suffix/weekKey', { wacchoi, date })
      closeWacchoiMenu()
      return
    }
    try {
      if (isWacchoiNgFor(wacchoi, date)) {
        await api.removeNgWacchoi({ suffix, board: fav.board, week_key: weekKey })
      } else {
        await api.addNgWacchoi({ suffix, board: fav.board, week_key: weekKey, wacchoi })
      }
      onngwacchoichange()
    } catch (e) {
      console.error('[ng-wacchoi]', e)
    }
    closeWacchoiMenu()
  }

  // Determine whether a res is NG by wacchoi (board + week-scoped suffix match).
  // isWacchoiNgFor returns false on null suffix/week (date parse failure), so a
  // non-wacchoi res or an unparseable date safely falls through to "not NG".
  function isWacchoiNg(r) {
    if (!wacchoiEnabledFlag) return false
    return isWacchoiNgFor(resolveWacchoi(r), r.date)
  }

  // --- Wacchoi search ---
  let wacchoiSearchLoading = $state(false)
  let wacchoiSearchResult = $state(null) // null = closed, [] = empty result, [...] = results
  let wacchoiSearchTarget = $state(null) // the suffix that was searched
  const wacchoiSearchHits = $derived(
    wacchoiSearchResult?.reduce((s, t) => s + t.res.length, 0) ?? 0,
  )

  async function startWacchoiSearch(wacchoi) {
    closeWacchoiMenu()
    const suffix = extractWacchoiSuffix(wacchoi)
    if (!suffix) return
    wacchoiSearchTarget = suffix
    wacchoiSearchLoading = true
    wacchoiSearchResult = null
    try {
      wacchoiSearchResult = await api.wacchoiSearch(fav.server, fav.board, suffix)
    } catch (e) {
      console.error('[wacchoi-search]', e)
      wacchoiSearchResult = []
    } finally {
      wacchoiSearchLoading = false
    }
  }

  function closeWacchoiSearch() {
    wacchoiSearchResult = null
    wacchoiSearchTarget = null
  }

  // --- ID right-click / long-press menu ---
  // Menu state: the ID string the menu acts on, or null (closed).
  let idMenu = $state(null)

  function openIdMenu(id) {
    idMenu = id
  }
  function closeIdMenu() {
    idMenu = null
  }

  // Long-press detection for the ID span (touch devices, 500ms, same threshold as FavoritesList).
  let idPressTimer
  let idLongPressed = false
  function onIdPointerDown(e, id) {
    if (e.pointerType !== 'touch') return
    idLongPressed = false
    idPressTimer = setTimeout(() => {
      idLongPressed = true
      openIdMenu(id)
    }, 500)
  }
  function cancelIdPress() {
    clearTimeout(idPressTimer)
  }
  // Handle left-click on the ID badge:
  //   - After a long-press (which already opened the menu), swallow the click.
  //   - Otherwise open the ID-list modal.
  function onIdClick(e, id) {
    if (idLongPressed) {
      idLongPressed = false
      e.stopPropagation()
      return
    }
    openIdList(id)
  }

  // Copy the ID string (with the "ID:" prefix) to the clipboard.
  async function copyId(id) {
    await copyText('ID:' + id)
    closeIdMenu()
  }

  // Add or remove the ID from the NG list. Calls onngchange to let the parent reload.
  async function toggleNg(id) {
    try {
      if (ngIds.has(id)) {
        await api.removeNgId(id)
      } else {
        await api.addNgId(id)
      }
      onngchange()
    } catch (e) {
      console.error('[ng]', e)
    }
    closeIdMenu()
  }

  // --- ID search ---
  let idSearchLoading = $state(false)
  let idSearchResult = $state(null) // null = closed, [] = empty result, [...] = results
  let idSearchTarget = $state(null) // the ID that was searched
  const idSearchHits = $derived(
    idSearchResult?.reduce((s, t) => s + t.res.length, 0) ?? 0,
  )

  async function startIdSearch(id) {
    closeIdMenu()
    idSearchTarget = id
    idSearchLoading = true
    idSearchResult = null
    try {
      idSearchResult = await api.idSearch(fav.server, fav.board, id)
    } catch (e) {
      console.error('[id-search]', e)
      idSearchResult = []
    } finally {
      idSearchLoading = false
    }
  }

  function closeIdSearch() {
    idSearchResult = null
    idSearchTarget = null
  }
</script>

<!-- body is already sanitized on the server. linkify makes anchors clickable. -->
{#snippet body(html)}
  <div class="body" role="presentation" onclick={onBodyClick}>{@html linkify(html)}</div>
{/snippet}

<!-- Back-references (reses that anchor to this res). Tap to follow. -->
{#snippet refs(num)}
  {@const list = backrefs.get(num)}
  {#if list}
    <div class="backrefs" role="presentation" onclick={onBodyClick}>
      {#each list as src}
        <span class="anchor" data-anchor={src}>&gt;&gt;{src}</span>
      {/each}
    </div>
  {/if}
{/snippet}

<!-- Res header + body, shared by every render site. -->
{#snippet resHead(r)}
  {@const stats = idStats.get(r.num)}
  {@const resolvedId = resolveId(r)}
  {@const wStats = wacchoiStats.get(r.num)}
  {@const wNameColorCls = wStats && wStats.total >= 2 ? `id-${wStats.colorLevel}` : ''}
  <span class="num">{r.num}</span><!--
       Name: when the thread has wacchoi enabled, linkifyWacchoi() wraps the
         wacchoi token inside a clickable .wacchoi-badge span.  The .name element
         inherits the colour class set by wNameColorCls so the badge takes the
         same colour without an extra wrapper.  Click/contextmenu/pointer events
         are delegated to .name span handlers (onBodyClick / onWacchoiContextMenu /
         onWacchoiPointerDown) which inspect e.target.closest('.wacchoi-badge'). -->
  {#if wacchoiEnabledFlag}
    <span
      class="name {wNameColorCls}"
      role="presentation"
      data-date={r.date}
      onclick={onBodyClick}
      oncontextmenu={onWacchoiContextMenu}
      onpointerdown={onWacchoiPointerDown}
      onpointerup={cancelWacchoiPress}
      onpointerleave={cancelWacchoiPress}
      onpointercancel={cancelWacchoiPress}
    >{@html linkifyWacchoi(r.name)}</span>
  {:else}
    <span class="name">{formatName(r.name)}</span>
  {/if}
  <span class="date">{stripId(r.date)}</span><!--
       ID badge: always shown when an ID exists (total>=2 only controls colour).
         Right-click (PC) or long-press (touch) opens the ID context menu.
         For total>=2 the span is coloured and shows order/total counts.
         The &nbsp; inside the {#if} separates the badge from .date only when an ID exists. -->
  {#if resolvedId}
    {@const colorCls = stats && stats.total >= 2 ? `id-${stats.colorLevel}` : 'id-l1'}
    {@const label = stats && stats.total >= 2
      ? `ID:${resolvedId} (${stats.order}/${stats.total})`
      : `ID:${resolvedId}`}
    &nbsp;<span
      class="id-badge {colorCls} resid"
      role="button"
      tabindex="0"
      data-id={resolvedId}
      oncontextmenu={(e) => { e.preventDefault(); openIdMenu(resolvedId) }}
      onpointerdown={(e) => onIdPointerDown(e, resolvedId)}
      onpointerup={cancelIdPress}
      onpointerleave={cancelIdPress}
      onpointercancel={cancelIdPress}
      onclick={(e) => onIdClick(e, resolvedId)}
      onkeydown={(e) => e.key === 'Enter' && openIdList(resolvedId)}
    >{label}</span>
  {/if}
{/snippet}

<!-- resHead + body snippet combined (used in main list). -->
{#snippet resHeadAndBody(r)}
  {@const ngd = (r.id && ngIds.has(r.id)) || isWacchoiNg(r)}
  {#if ngd}
    <!-- NG post: header shown struck-through + muted, body hidden completely. -->
    <del class="ng">
      {@render resHead(r)}
    </del>
  {:else}
    {@render resHead(r)}
    {@render body(r.body)}
  {/if}
{/snippet}

<!-- Res body (list): header + body + followable back-references. -->
{#snippet resBody(r)}
  {@render resHeadAndBody(r)}
  {@render refs(r.num)}
{/snippet}

<!-- Render a single res node inside the anchor tree at the given depth.
     depth is reflected as inline margin-left so getBoundingClientRect().left
     grows monotonically with depth (verifiable in E2E tests).
     highlight adds a visual border/background accent without affecting indentation. -->
{#snippet anchorResNode(num, depth = 0, highlight = false)}
  {@const r = resOf(num)}
  <div class="anchor-node" style="margin-left: {depth * 0.6}rem">
    {#if r.missing}
      <div class="res missing">レス {num} は未取得です</div>
    {:else}
      <!-- Uses resHeadAndBody so NG posts are hidden even inside the anchor tree. -->
      <div class="res" class:anchor-self={highlight}>
        {@render resHeadAndBody(r)}
      </div>
    {/if}
  </div>
{/snippet}

<!-- Sticky title header: stays visible while scrolling (replaces the removed
     back/update bar). Sits just below the global NavBar. -->
<h1 class="title" data-testid="thread-title">{data?.title || fav.title}</h1>

{#if refreshError}
  <p class="refresh-error" data-testid="refresh-error" role="alert">
    更新に失敗しました（表示は前回取得分です）: {refreshError}
  </p>
{/if}

{#if data}
  <div class="thread-body" use:backSwipe use:pullRefresh={() => ({
    enabled: !refreshing,
    isBlocked: isAnyModalOpen,
    onRefresh: triggerRefresh,
    onDrag: (px, phase) => { pullPx = px; pullPhase = phase },
  })}>
    {#each data.res as r (r.num)}
      <div class="res" use:track={r.num} class:unread={r.num > readBaseline}>
        {@render resBody(r)}
      </div>
    {/each}
  </div>

  <!-- Pull-to-refresh panel: slides up from the bottom as the user over-pulls.
       position:fixed keeps it out of the document flow (no layout shift).
       translateY(100%) hides it below the viewport; reduced by pullPx to reveal it. -->
  <div
    class="pull-refresh-panel"
    class:above-threshold={pullPx >= PULL_THRESHOLD_PX}
    data-testid="pull-refresh"
    style="transform: translateY({refreshing ? 0 : Math.max(0, 100 - pullPx)}%)"
    aria-hidden={pullPx === 0 && !refreshing}
  >
    {#if refreshing}
      <span class="pull-refresh-spinner" aria-label="更新中"></span>
      <span>更新中…</span>
    {:else if pullPhase === 'dragging' && pullPx >= PULL_THRESHOLD_PX}
      <span>離して更新</span>
    {:else}
      <span>更新する</span>
    {/if}
  </div>
{/if}

{#if anchorRoot != null}
  <Modal onclose={closeAnchor}>
    <!-- Clicking >>N inside the tree replaces the root (no stacking). -->
    <div role="presentation" onclick={onBodyClick}>
      {#if anchorTree != null}
        <!-- Single unified tree: ancestors (shallowest first) -> self -> descendants.
             depth drives indentation via inline margin-left; highlight is visual-only. -->
        {#each anchorTree as item (item.num)}
          {@render anchorResNode(item.num, item.depth, item.highlight ?? false)}
        {/each}
      {/if}
    </div>
  </Modal>
{/if}

<!-- ID list modal: all reses in this thread posted by the tapped ID. -->
{#if idListId != null}
  <Modal onclose={closeIdList}>
    {#snippet header()}
      <div class="menu-title">ID:{idListId}（{idListRes.length}件）</div>
    {/snippet}
    <div class="id-list" data-testid="id-list" role="presentation" onclick={onBodyClick}>
      {#each idListRes as r (r.num)}
        <div class="res id-list-res">
          {@render resHeadAndBody(r)}
        </div>
      {/each}
    </div>
  </Modal>
{/if}

<!-- Wacchoi list modal: all reses in this thread posted under the same wacchoi. -->
{#if wacchoiListKey != null}
  <Modal onclose={closeWacchoiList}>
    {#snippet header()}
      <div class="menu-title">ﾜｯﾁｮｲ:{wacchoiListKey}（{wacchoiListRes.length}件）</div>
    {/snippet}
    <div class="id-list" data-testid="wacchoi-list" role="presentation" onclick={onBodyClick}>
      {#each wacchoiListRes as r (r.num)}
        <div class="res id-list-res">
          {@render resHeadAndBody(r)}
        </div>
      {/each}
    </div>
  </Modal>
{/if}

<!-- Wacchoi right-click / long-press menu modal. -->
{#if wacchoiMenu != null}
  <Modal onclose={closeWacchoiMenu}>
    {#snippet header()}
      <div class="menu-title">ﾜｯﾁｮｲ:{wacchoiMenu.wacchoi}</div>
    {/snippet}
    <div class="menu" data-testid="wacchoi-menu">
      <button class="action" onclick={() => toggleNgWacchoi(wacchoiMenu.wacchoi, wacchoiMenu.date)}>
        {isWacchoiNgFor(wacchoiMenu.wacchoi, wacchoiMenu.date) ? 'NGﾜｯﾁｮｲから削除' : 'NGﾜｯﾁｮｲに追加'}
      </button>
      <button class="action" onclick={() => copyWacchoi(wacchoiMenu.wacchoi)}>コピー</button>
      <button class="action" onclick={() => startWacchoiSearch(wacchoiMenu.wacchoi)}>取得済みスレから検索</button>
    </div>
  </Modal>
{/if}

<!-- Shared board-search result modal (used by both ID and wacchoi search).
     `loading`/`result`/`hits` are the per-search state; `label` is the term shown
     in the title; `testid` keys the result container for E2E. -->
{#snippet searchResultModal(loading, result, hits, label, testid, onclose)}
  {#if loading || result != null}
    <Modal {onclose}>
      {#snippet header()}
        <div class="menu-title">
          {#if loading}
            {label} を検索中…
          {:else}
            {label} の検索結果（{hits}件）
          {/if}
        </div>
      {/snippet}
      {#if !loading}
        <div class="search-result" data-testid={testid}>
          {#if result.length === 0}
            <p class="search-empty">該当なし</p>
          {:else}
            {#each result as thread (thread.thread_id)}
              <h3 class="search-thread-title">{thread.title}</h3>
              {#each thread.res as r (r.num)}
                <div class="res search-res">
                  <span class="num">{r.num}</span>
                  <span class="name">{formatName(r.name)}</span>
                  <span class="date">{r.date}</span>
                  <div class="body" role="presentation">{@html linkify(r.body)}</div>
                </div>
              {/each}
            {/each}
          {/if}
        </div>
      {/if}
    </Modal>
  {/if}
{/snippet}

<!-- Wacchoi search modal. -->
{@render searchResultModal(
  wacchoiSearchLoading,
  wacchoiSearchResult,
  wacchoiSearchHits,
  `ﾜｯﾁｮｲ末尾:${wacchoiSearchTarget}`,
  'wacchoi-search-result',
  closeWacchoiSearch,
)}

<!-- ID right-click / long-press menu modal. -->
{#if idMenu != null}
  <Modal onclose={closeIdMenu}>
    {#snippet header()}
      <div class="menu-title">ID:{idMenu}</div>
    {/snippet}
    <div class="menu" data-testid="id-menu">
      <button class="action" onclick={() => toggleNg(idMenu)}>
        {ngIds.has(idMenu) ? 'NGIDから削除' : 'NGIDに追加'}
      </button>
      <button class="action" onclick={() => copyId(idMenu)}>コピー</button>
      <button class="action" onclick={() => startIdSearch(idMenu)}>取得済みスレから検索</button>
    </div>
  </Modal>
{/if}

<!-- ID search modal. -->
{@render searchResultModal(
  idSearchLoading,
  idSearchResult,
  idSearchHits,
  `ID:${idSearchTarget}`,
  'id-search-result',
  closeIdSearch,
)}

<style>
  /* Sticky title header. Sits below the global NavBar (sticky at top:0,
     height = --navbar-h). On PC (>=768px) the detail-pane is the scroll
     container so NavBar no longer overlaps the pane content — reset top to 0. */
  .title {
    position: sticky;
    top: var(--navbar-h, 3.2rem);
    z-index: 5;
    margin: 0;
    padding: 0.5rem 0;
    font-size: 1.05rem;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @media (min-width: 768px) {
    .title {
      top: 0;
    }
  }
  .res {
    background: var(--card-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.5rem;
    margin-bottom: 0.3rem;
  }
  .res.unread {
    border-left: 3px solid var(--danger);
  }
  .num {
    font-weight: bold;
    color: var(--name);
  }
  .name {
    color: var(--name);
    margin-left: 0.3rem;
  }
  .date {
    font-size: 0.75rem;
    color: var(--muted);
    margin-left: 0.3rem;
  }
  .body {
    margin-top: 0.3rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  :global(.anchor) {
    color: var(--link);
    cursor: pointer;
    text-decoration: underline;
  }
  /* Inline wacchoi badge: inherits colour from .name so that per-res colour
     classes (id-l2..l5) applied to .name propagate without extra wrappers.
     Clickable-badge affordance (cursor/non-selectable) is shared with .resid below. */
  :global(.wacchoi-badge) {
    color: inherit;
  }
  .backrefs {
    margin-top: 0.3rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    font-size: 0.8rem;
  }
  /* Anchor tree: depth-driven indentation via inline margin-left (set per node).
     A left border on each indented node gives the visual tree guide. */
  .anchor-node {
    /* padding-left leaves room for the border without shifting text too much */
    padding-left: 0.5rem;
    border-left: 2px solid var(--border);
  }
  /* Tighter vertical spacing inside the tree for scannability. */
  .anchor-node .res {
    margin-bottom: 0.2rem;
  }
  .anchor-node .res.missing {
    color: var(--muted);
    font-size: 0.85rem;
  }
  /* Highlight the pivot res (the clicked N) in the anchor tree.
     border-left here overrides the node-level border and gives a coloured accent;
     indentation is unaffected (it comes from margin-left on .anchor-node). */
  .anchor-node .res.anchor-self {
    border-left: 3px solid var(--accent, var(--link));
    background: var(--highlight-bg, var(--card-bg));
  }
  .refresh-error {
    margin: 0.4rem 0;
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
    color: var(--danger);
    background: var(--error-bg);
    border-radius: 4px;
  }
  /* ID/wacchoi badge: same font-size as surrounding .date text.
     Gap from the preceding element comes from an &nbsp; placed inside
     each {#if} block (just before the span), so it only renders when the badge is shown. */
  .id-badge {
    font-size: 0.75rem;
  }
  /* id-l1: single-occurrence ID — shown as muted text (clickable but no colour accent). */
  .id-l1 { color: var(--muted); }
  .id-l2 { color: var(--id-l2); }
  .id-l3 { color: var(--id-l3); }
  .id-l4 { color: var(--id-l4); }
  .id-l5 { color: var(--id-l5); font-weight: bold; }

  /* Clickable badge affordance, shared by the ID badge and the wacchoi badge. */
  .resid,
  :global(.wacchoi-badge) {
    cursor: pointer;
    -webkit-touch-callout: none;
    user-select: none;
  }

  /* NG post: header is struck-through + muted; body is not rendered. */
  .ng {
    color: var(--muted);
    text-decoration: line-through;
  }

  /* ID action menu (same layout as FavoritesList menu). */
  .menu {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    width: 16rem;
    max-width: 100%;
  }
  .menu-title {
    font-weight: 600;
    word-break: break-all;
    font-size: 0.9rem;
  }
  .action {
    padding: 0.7rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    text-align: center;
    font-size: 0.95rem;
  }

  /* ID list modal content (same-ID reses in the current thread). */
  .id-list {
    min-width: min(32rem, 90vw);
    max-width: 90vw;
  }
  .id-list-res {
    font-size: 0.9rem;
  }

  /* ID search result modal content. */
  .search-result {
    min-width: min(32rem, 90vw);
    max-width: 90vw;
  }
  .search-thread-title {
    font-size: 0.9rem;
    color: var(--accent);
    margin: 0.8rem 0 0.3rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0.2rem;
  }
  .search-res {
    font-size: 0.9rem;
  }
  .search-empty {
    color: var(--muted);
  }

  /* Pull-to-refresh panel: fixed to the bottom of the viewport, slides up on over-pull.
     transform is driven inline by pullPx. z-index sits above thread content but
     below modals (z-index:50). */
  .pull-refresh-panel {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 4rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    font-size: 0.95rem;
    background: var(--card-bg);
    border-top: 1px solid var(--border);
    color: var(--muted);
    z-index: 20;
    /* Start hidden below viewport; translateY is driven inline. */
    will-change: transform;
    user-select: none;
    pointer-events: none;
  }
  /* Highlight text when past the release threshold. */
  .pull-refresh-panel.above-threshold {
    color: var(--accent);
    font-weight: 600;
  }
  /* Simple CSS spinner (no extra dependencies). */
  .pull-refresh-spinner {
    display: inline-block;
    width: 1.1rem;
    height: 1.1rem;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: pr-spin 0.7s linear infinite;
  }
  @keyframes pr-spin {
    to { transform: rotate(360deg); }
  }
</style>
