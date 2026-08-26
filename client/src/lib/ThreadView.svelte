<script>
  import { onDestroy, untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'
  import { formatName } from './name.js'
  import { stripId, buildIdStats } from './id.js'
  import {
    buildWacchoiStats,
    wacchoiEnabled,
    linkifyWacchoi,
    extractWacchoiSuffix,
    wacchoiWeekKey,
  } from './wacchoi.js'
  import { linkify, extractImageUrls, ANCHOR_RE } from './linkify.js'
  import { scopedTo, findNgWord, isValidNgWord } from './ng.js'
  import { resBodyText } from './res-text.js'
  import { copyText } from './clipboard.js'
  import Modal from './Modal.svelte'
  import ImageViewer from './ImageViewer.svelte'
  import Icon from './Icon.svelte'

  let {
    fav,
    onback,
    onprogress = () => {},
    ngIds = [],
    onngchange = () => {},
    ngWords = [],
    onngwordchange = () => {},
    ngWacchoi = [],
    onngwacchoichange = () => {},
  } = $props()

  // NG IDs and NG words are stored per (server, board), so only this board's rules can
  // hide a post here. Narrowing once keeps every per-res check board-scoped by
  // construction — the same ID string on another board never matches.
  const boardNgIds = $derived(new Set(scopedTo(ngIds, fav.server, fav.board).map((r) => r.ng_id)))
  const boardNgWords = $derived(scopedTo(ngWords, fav.server, fav.board))

  let data = $state(null)
  // Newest-first timeline. The entry batch ends at the previous-read post;
  // older posts are appended below it during idle time.
  let visibleRes = $state([])
  let readBoundaryNum = $state(null)
  let olderComplete = $state(true)
  let olderQueue = []
  let olderIdleHandle = null
  const OLDER_BATCH_SIZE = 50
  // Surfaced when a manual refresh (footer button or post-write reload) fails.
  // The stored dat is still shown below, so this is a non-blocking notice rather
  // than a hard error. Entry never reloads, so it cannot set this on open.
  let refreshError = $state(null)
  // true while a manual refresh triggered by the footer button is in flight.
  // Used to prevent double-firing and to disable the button.
  let refreshing = $state(false)
  // Read position (max res number that has passed through the viewport). Initialized from the saved read_res (only on first mount).
  let maxRead = $state(untrack(() => fav.read_res))
  // Unread-bar baseline: reses with num > readBaseline show the unread left-border.
  // Initialized to fav.read_res like maxRead. Raised on pull-to-refresh so that
  // pre-refresh reses lose their bar and only newly-added reses stay unread.
  let readBaseline = $state(untrack(() => fav.read_res))
  // Restore-on-open guard: scroll to the saved read position only after the first
  // successful load, never after a manual reload.
  let restored = $state(false)
  let trackingReady = false
  let threadBody
  let destroyed = false
  let restoreFrame = null
  let settleFrame = null
  let refreshFrame = null

  function cancelOlderWork() {
    if (olderIdleHandle == null) return
    if ('cancelIdleCallback' in window) window.cancelIdleCallback(olderIdleHandle)
    else clearTimeout(olderIdleHandle)
    olderIdleHandle = null
  }

  function scheduleOlderBatch() {
    if (destroyed || olderComplete || olderIdleHandle != null) return
    const append = () => {
      olderIdleHandle = null
      if (destroyed) return
      appendOlderBatch()
      if (!olderComplete) scheduleOlderBatch()
    }
    if ('requestIdleCallback' in window) {
      olderIdleHandle = window.requestIdleCallback(append, { timeout: 500 })
    } else {
      olderIdleHandle = setTimeout(append, 0)
    }
  }

  function appendOlderBatch() {
    const next = olderQueue.splice(0, OLDER_BATCH_SIZE)
    if (next.length === 0) return false
    visibleRes = [...visibleRes, ...next]
    olderComplete = olderQueue.length === 0
    return true
  }

  function finishInitialView() {
    trackingReady = true
    observer?.disconnect()
    threadBody?.querySelectorAll(':scope > .res').forEach((node) => observer?.observe(node))
    scheduleOlderBatch()
  }

  function settleInitialView(boundary) {
    if (destroyed || !threadBody || !boundary.isConnected) return
    const bodyRect = threadBody.getBoundingClientRect()
    const boundaryBelowViewport = boundary.getBoundingClientRect().bottom > bodyRect.bottom

    if (boundaryBelowViewport) {
      boundary.scrollIntoView({ block: 'end', behavior: 'instant' })
      finishInitialView()
      return
    }

    // When the new section is shorter than the viewport, fill the remaining
    // space with real older posts. Keep the viewport at the newest end instead
    // of manufacturing blank space above it with a CSS spacer.
    if (threadBody.scrollHeight <= threadBody.clientHeight && appendOlderBatch()) {
      settleFrame = requestAnimationFrame(() => {
        settleFrame = null
        settleInitialView(boundary)
      })
      return
    }

    threadBody.scrollTop = 0
    finishInitialView()
  }

  function prepareTimeline(nextData) {
    cancelOlderWork()
    const newestFirst = nextData.res.toReversed()
    if (newestFirst.length === 0) {
      visibleRes = []
      olderQueue = []
      readBoundaryNum = null
      olderComplete = true
      return
    }

    // Include the previous-read post itself in the urgent batch. A stale saved
    // position beyond the available dat safely falls back to the newest post.
    const foundBoundary = newestFirst.findIndex((r) => r.num <= readBaseline)
    const boundaryIndex = readBaseline < 1 ? newestFirst.length - 1 : Math.max(0, foundBoundary)
    readBoundaryNum = newestFirst[boundaryIndex].num
    visibleRes = newestFirst.slice(0, boundaryIndex + 1)
    olderQueue = newestFirst.slice(boundaryIndex + 1)
    olderComplete = olderQueue.length === 0
  }

  onDestroy(() => {
    destroyed = true
    cancelOlderWork()
    if (restoreFrame != null) cancelAnimationFrame(restoreFrame)
    if (settleFrame != null) cancelAnimationFrame(settleFrame)
    if (refreshFrame != null) cancelAnimationFrame(refreshFrame)
    observer?.disconnect()
  })
  // Post modal: open/close and form state.
  let postModalOpen = $state(false)
  let postMessage = $state('')
  let postName = $state('')
  let postMail = $state('sage')
  let postError = $state(null)
  let postSubmitting = $state(false)

  // Render the stored dat only (GET dat, no 5ch access). Used on entry (ChMate
  // model: opening a thread never touches 5ch — the list bulk-refresh keeps the
  // stored dat fresh) and as the second half of reloadAndFetch.
  async function fetchDat() {
    const nextData = await api.getDat(fav.server, fav.board, fav.thread_id)
    prepareTimeline(nextData)
    data = nextData
    if (restored) {
      if (refreshFrame != null) cancelAnimationFrame(refreshFrame)
      refreshFrame = requestAnimationFrame(() => {
        refreshFrame = null
        if (!destroyed) scheduleOlderBatch()
      })
    }
    if (data.read_res > maxRead) maxRead = data.read_res
    // Update the mosaic URL set from the server response.
    mosaicUrls = new Set(data.mosaic_urls ?? [])
  }

  // Reload from 5ch (GET reload: checks subject.txt's res_count and pulls new dat
  // if it grew), then render the updated dat. Only invoked by the footer refresh
  // button and after a post write — never on entry.
  async function reloadAndFetch() {
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
    await fetchDat()
  }

  // Manual refresh triggered by the footer refresh button.
  // Guards against double-fire via the `refreshing` flag.
  // Before loading, advance readBaseline (and maxRead) to the last visible res
  // number so that all currently-shown reses lose their unread bar and only
  // newly-added reses (num > old last) will show it after the fetch.
  async function triggerRefresh() {
    if (refreshing) return
    refreshing = true
    // Advance baseline to the last res currently in data (everything already
    // fetched is considered read).
    const lastNum = data?.res?.length ? data.res[data.res.length - 1].num : maxRead
    readBaseline = Math.max(readBaseline, lastNum)
    maxRead = Math.max(maxRead, lastNum)
    try {
      await reloadAndFetch()
    } finally {
      refreshing = false
    }
  }

  // Returns true when any modal/overlay is open. Used by backSwipe
  // to suppress gestures while overlays are in front.
  function isAnyModalOpen() {
    return (
      postModalOpen ||
      anchorRoot != null ||
      idListId != null ||
      idMenu != null ||
      wacchoiListKey != null ||
      wacchoiMenu != null ||
      ngMenu != null ||
      ngWordForm != null ||
      replyMenuResNum != null ||
      idSearchLoading ||
      idSearchResult != null ||
      wacchoiSearchLoading ||
      wacchoiSearchResult != null ||
      imageMenu != null ||
      imageViewerState != null
    )
  }

  // Submit the post modal: write to 5ch, reset the form on success, then reload
  // so the new res (marked own = pink) appears. Errors stay in the modal.
  async function submitPost() {
    postError = null
    postSubmitting = true
    try {
      await api.post(fav.server, fav.board, fav.thread_id, {
        message: postMessage,
        name: postName || undefined,
        mail: postMail || undefined,
      })
      postMessage = ''
      postName = ''
      postMail = 'sage'
      postModalOpen = false
      // Reload so the new res (marked own = pink) appears: post_message writes to
      // 5ch and records own_posts but does not update the stored dat, so a plain
      // fetchDat would not show the just-written post until a manual refresh.
      await reloadAndFetch()
    } catch (e) {
      postError = e.message
    } finally {
      postSubmitting = false
    }
  }

  // Entry (ChMate model): render the stored dat immediately with no 5ch access.
  // The restore effect runs after this single fetch. Updates are delegated to the
  // list bulk-refresh and the footer refresh button, never to entry.
  $effect(() => {
    fetchDat()
  })

  // If the new section fills the viewport, place its boundary at the bottom.
  // Otherwise show enough real older posts to fill the viewport naturally.
  $effect(() => {
    if (!data || restored) return
    restored = true
    restoreFrame = requestAnimationFrame(() => {
      restoreFrame = null
      if (destroyed || !threadBody) return
      const boundary = threadBody.querySelector('[data-testid="read-boundary"]')
      if (!boundary) {
        // An empty stored dat can later gain posts through manual refresh. Keep
        // progress tracking live so those newly-created cards are observed.
        trackingReady = true
        return
      }
      settleFrame = requestAnimationFrame(() => {
        settleFrame = null
        settleInitialView(boundary)
      })
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
        if (trackingReady && e.isIntersecting) {
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
    const sendBeacon = () => beaconProgress(fav.server, fav.board, fav.thread_id, maxRead)
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

  // In-body anchor >>N (imported from linkify.js — single source of truth for both
  // the backref builder and the anchor-tree walk).

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
  const wacchoiStats = $derived.by(() => buildWacchoiStats(data?.res ?? [], wacchoiEnabledFlag))

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

    return [...ancestors, { num: anchorRoot, depth: selfDepth, highlight: true }, ...descendants]
  })

  // Body click (shared by list and modal). Follow the anchor or open wacchoi list when tapped.
  function onBodyClick(e) {
    // After a card long-press opened the reply menu, swallow the trailing click so it
    // does not follow an anchor / open a list on top of the menu.
    if (cardLongPressed) {
      cardLongPressed = false
      e.stopPropagation()
      return
    }
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

  // --- Image mosaic state ---
  // Set of URLs with mosaic=1 for this thread (sourced from DatResponse.mosaic_urls).
  let mosaicUrls = $state(new Set())

  // --- Image viewer state ---
  // When open: { images: [...], initialIndex: number }
  // images item: { href, path, url, resNum, indexInRes, globalIndex }
  let imageViewerState = $state(null)

  // Flat list of all images across all res entries (built reactively from data).
  const allImages = $derived.by(() => {
    if (!data?.res) return []
    const out = []
    for (const r of data.res) {
      const imgs = extractImageUrls(r.body)
      imgs.forEach((img, indexInRes) => {
        out.push({ ...img, resNum: r.num, indexInRes, globalIndex: out.length })
      })
    }
    return out
  })

  function openImageViewer(resNum, indexInRes) {
    const idx = allImages.findIndex((img) => img.resNum === resNum && img.indexInRes === indexInRes)
    if (idx === -1) return
    imageViewerState = { images: allImages, initialIndex: idx }
  }

  // --- Image context menu ---
  // { url: string, mosaic: boolean } | null
  let imageMenu = $state(null)

  // Long-press detection for thumbnails (touch, 500ms).
  let imagePressTimer
  let imageLongPressed = false
  function onThumbPointerDown(e, url) {
    e.stopPropagation()
    if (e.pointerType !== 'touch') return
    imageLongPressed = false
    imagePressTimer = setTimeout(() => {
      imageLongPressed = true
      imageMenu = { url, mosaic: mosaicUrls.has(url) }
    }, 500)
  }
  function cancelThumbPress() {
    clearTimeout(imagePressTimer)
  }

  // Toggle mosaic for a URL and persist to the API.
  async function toggleMosaic(url) {
    const newMosaic = !mosaicUrls.has(url)
    try {
      if (newMosaic) {
        await api.setImageMosaic(url)
        mosaicUrls.add(url)
      } else {
        await api.unsetImageMosaic(url)
        mosaicUrls.delete(url)
      }
      mosaicUrls = new Set(mosaicUrls) // trigger reactivity
    } catch (e) {
      console.error('[image-mosaic]', e)
    }
    imageMenu = null
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
    e.stopPropagation()
    const date = resDateFromTarget(badge)
    openWacchoiMenu(badge.dataset.wacchoi, date)
  }

  // Long-press detection for the wacchoi badge (touch devices, 500ms, same pattern as ID).
  let wacchoiPressTimer
  let wacchoiLongPressed = false
  function onWacchoiPointerDown(e) {
    const badge = e.target.closest('.wacchoi-badge')
    if (!badge) return
    e.stopPropagation()
    if (e.pointerType !== 'touch') return
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

  // Add the wacchoi to the NG list. Removal belongs exclusively to the NG post menu.
  async function addNgWacchoi(wacchoi, date) {
    const suffix = extractWacchoiSuffix(wacchoi)
    const weekKey = wacchoiWeekKey(date)
    if (!suffix || !weekKey) {
      console.error('[ng-wacchoi] cannot compute suffix/weekKey', { wacchoi, date })
      closeWacchoiMenu()
      return
    }
    try {
      await api.addNgWacchoi({ suffix, board: fav.board, week_key: weekKey, wacchoi })
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

  // Return the single reason presented for an NG post, in a fixed precedence order.
  // When a post matches several lists only the first is shown; removing it then reveals
  // the next reason after the parent reloads the NG state.
  function ngReason(r) {
    const id = resolveId(r)
    if (id && boardNgIds.has(id)) {
      return { kind: 'id', label: 'NG ID', id }
    }
    const wacchoi = resolveWacchoi(r)
    if (isWacchoiNg(r)) {
      return { kind: 'wacchoi', label: 'NGワッチョイ', wacchoi, date: r.date }
    }
    // Matched against the display text, so a rule reads the same as what is on screen
    // (no markup, entities decoded, <br> as newlines).
    const word = findNgWord(resBodyText(r.body), boardNgWords)
    if (word) {
      return { kind: 'word', label: 'NG Word', word }
    }
    return null
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
    e.stopPropagation()
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

  // Add the ID to this board's NG list. Removal belongs exclusively to the NG post menu.
  async function addNg(id) {
    try {
      await api.addNgId({ server: fav.server, board: fav.board, ng_id: id })
      onngchange()
    } catch (e) {
      console.error('[ng]', e)
    }
    closeIdMenu()
  }

  // --- NG post disclosure / context menu ---
  let expandedNgRes = $state(new Set())
  let ngMenu = $state(null) // { resNum, kind, label, ...reason fields } | null

  function toggleNgBody(e, resNum) {
    e.preventDefault()
    e.stopPropagation()
    // A touch long-press is followed by a synthetic click. Consume it so opening
    // the menu never also reveals the body underneath.
    if (cardLongPressed) {
      cardLongPressed = false
      return
    }
    if (expandedNgRes.has(resNum)) expandedNgRes.delete(resNum)
    else expandedNgRes.add(resNum)
    expandedNgRes = new Set(expandedNgRes)
  }

  function closeNgMenu() {
    ngMenu = null
    cardLongPressed = false
  }

  async function removeNgReason() {
    if (ngMenu == null) return
    const target = ngMenu
    try {
      if (target.kind === 'id') {
        await api.removeNgId({ server: fav.server, board: fav.board, ng_id: target.id })
        onngchange()
      } else if (target.kind === 'word') {
        await api.removeNgWord(target.word)
        onngwordchange()
      } else {
        const suffix = extractWacchoiSuffix(target.wacchoi)
        const weekKey = wacchoiWeekKey(target.date)
        if (!suffix || !weekKey) throw new Error('NGワッチョイの対象を特定できません')
        await api.removeNgWacchoi({ suffix, board: fav.board, week_key: weekKey })
        onngwacchoichange()
      }
      expandedNgRes.delete(target.resNum)
      expandedNgRes = new Set(expandedNgRes)
    } catch (e) {
      console.error('[ng-remove]', e)
    }
    closeNgMenu()
  }

  // --- Reply context menu ---
  // The res number the reply menu is open for, or null (closed).
  let replyMenuResNum = $state(null)

  // Right-click anywhere on a res card opens its reply menu. Controls with a
  // dedicated context menu stop propagation before this card-level handler.
  function onCardContextMenu(e, r) {
    if (r?.num == null) return
    e.preventDefault()
    const reason = ngReason(r)
    if (reason) ngMenu = { resNum: r.num, ...reason }
    else replyMenuResNum = r.num
  }

  // Long-press detection for the whole res card (touch devices, 500ms, same pattern as
  // the ID/wacchoi/thumbnail long-press). Required because suppressing selection
  // via CSS prevents `contextmenu` from firing on some touch browsers, so the
  // native right-click path (onCardContextMenu) cannot cover touch.
  let cardPressTimer
  let cardLongPressed = false
  function onCardPointerDown(e, r) {
    if (e.pointerType !== 'touch') return
    if (r?.num == null) return
    cardLongPressed = false
    cardPressTimer = setTimeout(() => {
      cardLongPressed = true
      const reason = ngReason(r)
      if (reason) ngMenu = { resNum: r.num, ...reason }
      else replyMenuResNum = r.num
    }, 500)
  }
  function cancelCardPress() {
    clearTimeout(cardPressTimer)
  }

  function closeReplyMenu() {
    replyMenuResNum = null
    cardLongPressed = false
  }

  // --- NG word modal ---
  // Opened from the reply menu. `pattern` starts as the res's display text and is fully
  // editable; `alsoId` additionally registers the poster's ID for this same board.
  // null = closed.
  let ngWordForm = $state(null)

  function openNgWordForm(num) {
    closeReplyMenu()
    const r = resOf(num)
    ngWordForm = {
      kind: 'text',
      pattern: resBodyText(r?.body),
      // No ID on this post (some boards' OP) means there is nothing to also register,
      // so the checkbox is disabled and off.
      id: resolveId(r),
      alsoId: resolveId(r) != null,
      error: null,
      submitting: false,
    }
  }

  function closeNgWordForm() {
    ngWordForm = null
  }

  // Save the rule for this board, plus the poster's ID when the checkbox is on.
  // The pattern is validated by the same RegExp engine that will evaluate it, so a rule
  // that cannot be saved is exactly a rule that could never have matched.
  async function submitNgWord() {
    if (ngWordForm == null || ngWordForm.submitting) return
    const { kind, pattern, alsoId, id } = ngWordForm
    if (!pattern) {
      ngWordForm.error = 'パターンを入力してください'
      return
    }
    if (!isValidNgWord(kind, pattern)) {
      ngWordForm.error = '正規表現として解釈できません'
      return
    }
    ngWordForm.submitting = true
    ngWordForm.error = null
    try {
      await api.addNgWord({ server: fav.server, board: fav.board, kind, pattern })
      onngwordchange()
      if (alsoId && id) {
        await api.addNgId({ server: fav.server, board: fav.board, ng_id: id })
        onngchange()
      }
      closeNgWordForm()
    } catch (e) {
      console.error('[ng-word]', e)
      ngWordForm.error = e.message
      ngWordForm.submitting = false
    }
  }

  // Copy the res body as plain text to the clipboard, then close the reply menu.
  // Compensates for the touch selection loss caused by the selection-suppress CSS.
  // resBodyText turns the sanitized body HTML into the text on screen; NG word
  // matching uses the same conversion so a copied body and a saved rule agree.
  async function copyBody(num) {
    await copyText(resBodyText(resOf(num)?.body))
    closeReplyMenu()
  }

  // Open the post modal pre-filled with >>num on line 1, cursor on line 2.
  function startReply(num) {
    closeReplyMenu()
    postMessage = `>>${num}\n`
    postMail = postMail || 'sage'
    postError = null
    postModalOpen = true
    requestAnimationFrame(() => {
      const ta = document.querySelector('.post-textarea')
      if (ta) {
        ta.focus()
        const pos = postMessage.length
        ta.setSelectionRange(pos, pos)
      }
    })
  }

  // --- ID search ---
  let idSearchLoading = $state(false)
  let idSearchResult = $state(null) // null = closed, [] = empty result, [...] = results
  let idSearchTarget = $state(null) // the ID that was searched
  const idSearchHits = $derived(idSearchResult?.reduce((s, t) => s + t.res.length, 0) ?? 0)

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

<!-- body is already sanitized on the server. linkify makes anchors clickable and URLs into links. -->
<!-- resNum is used by the surrounding card's reply-menu handlers. -->
{#snippet body(html, resNum)}
  {@const images = extractImageUrls(html)}
  <div class="body" role="presentation" onclick={onBodyClick}>{@html linkify(html)}</div>
  {#if images.length > 0}
    <div class="thumb-strip">
      {#each images as img, indexInRes}
        {@const isMosaic = mosaicUrls.has(img.url)}
        <button
          class="thumb-btn"
          onclick={(e) => {
            if (imageLongPressed) {
              imageLongPressed = false
              e.stopPropagation()
              return
            }
            openImageViewer(resNum, indexInRes)
          }}
          oncontextmenu={(e) => {
            e.preventDefault()
            e.stopPropagation()
            imageMenu = { url: img.url, mosaic: isMosaic }
          }}
          onpointerdown={(e) => onThumbPointerDown(e, img.url)}
          onpointerup={cancelThumbPress}
          onpointerleave={cancelThumbPress}
          onpointercancel={cancelThumbPress}
          aria-label="画像を表示"
        >
          <img
            src="/api/images/{img.path}"
            alt="画像"
            class="thumb"
            class:thumb-mosaic={isMosaic}
            loading="lazy"
            onerror={(e) => e.currentTarget.classList.add('thumb-missing')}
          />
          <!-- Failed-load placeholder cross: shown via .thumb-missing + .thumb-error. -->
          <span class="thumb-error" aria-hidden="true"><Icon name="x" size="20" /></span>
        </button>
      {/each}
    </div>
  {/if}
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
  <span class="num">{r.num}</span
  ><!--
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
      onpointercancel={cancelWacchoiPress}>{@html linkifyWacchoi(r.name)}</span
    >
  {:else}
    <span class="name">{formatName(r.name)}</span>
  {/if}
  <span class="date">{stripId(r.date)}</span
  ><!--
       ID badge: always shown when an ID exists (total>=2 only controls colour).
         Right-click (PC) or long-press (touch) opens the ID context menu.
         For total>=2 the span is coloured and shows order/total counts.
         The &nbsp; inside the {#if} separates the badge from .date only when an ID exists. -->
  {#if resolvedId}
    {@const colorCls = stats && stats.total >= 2 ? `id-${stats.colorLevel}` : 'id-l1'}
    {@const label =
      stats && stats.total >= 2
        ? `ID:${resolvedId} (${stats.order}/${stats.total})`
        : `ID:${resolvedId}`}
    &nbsp;<span
      class="id-badge {colorCls} resid"
      role="button"
      tabindex="0"
      data-id={resolvedId}
      oncontextmenu={(e) => {
        e.preventDefault()
        e.stopPropagation()
        openIdMenu(resolvedId)
      }}
      onpointerdown={(e) => onIdPointerDown(e, resolvedId)}
      onpointerup={cancelIdPress}
      onpointerleave={cancelIdPress}
      onpointercancel={cancelIdPress}
      onclick={(e) => onIdClick(e, resolvedId)}
      onkeydown={(e) => e.key === 'Enter' && openIdList(resolvedId)}>{label}</span
    >
  {/if}
{/snippet}

<!-- resHead + body snippet combined (used in main list). -->
{#snippet resHeadAndBody(r)}
  {@const reason = ngReason(r)}
  {#if reason}
    <!-- NG post: concise struck-through disclosure; body starts hidden. -->
    <button
      class="ng-toggle"
      type="button"
      aria-expanded={expandedNgRes.has(r.num)}
      onclick={(e) => toggleNgBody(e, r.num)}><del class="ng">{r.num} {reason.label}</del></button
    >
    {#if expandedNgRes.has(r.num)}
      {@render body(r.body, r.num)}
    {/if}
  {:else}
    {@render resHead(r)}
    {@render body(r.body, r.num)}
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
      <div
        class="res"
        class:anchor-self={highlight}
        role="group"
        aria-label="レス {r.num}"
        oncontextmenu={(e) => onCardContextMenu(e, r)}
        onpointerdown={(e) => onCardPointerDown(e, r)}
        onpointerup={cancelCardPress}
        onpointerleave={cancelCardPress}
        onpointercancel={cancelCardPress}
      >
        {@render resHeadAndBody(r)}
      </div>
    {/if}
  </div>
{/snippet}

<!-- Thread view: flex column filling the full height of .detail-pane (PC) or dvh (phone).
     Structure: sticky header / scrollable body / fixed footer. -->
<div class="thread-view">
  <!-- Sticky title header. -->
  <h1 class="title" data-testid="thread-title">{data?.title || fav.title}</h1>

  {#if refreshError}
    <p class="refresh-error error" data-testid="refresh-error" role="alert">
      更新に失敗しました（表示は前回取得分です）: {refreshError}
    </p>
  {/if}

  <!-- Scrollable body: flex:1 so it fills remaining space between header and footer. -->
  <div class="thread-body" bind:this={threadBody} use:backSwipe>
    {#if data}
      {#if visibleRes.length > 0}
        <div class="read-bar" data-testid="thread-end" aria-hidden="true">
          <span class="read-bar-label">おわり</span>
        </div>
      {/if}
      {#each visibleRes as r (r.num)}
        {#if r.num === readBoundaryNum && readBaseline >= 1}
          <div class="read-bar" data-testid="read-boundary" aria-hidden="true">
            <span class="read-bar-label">前回ここまで</span>
          </div>
        {/if}
        <!-- own takes priority over unread: when r.own is true, unread class is not added -->
        <div
          class="res"
          use:track={r.num}
          class:unread={r.num > readBaseline && !r.own}
          class:own={r.own}
          role="group"
          aria-label="レス {r.num}"
          oncontextmenu={(e) => onCardContextMenu(e, r)}
          onpointerdown={(e) => onCardPointerDown(e, r)}
          onpointerup={cancelCardPress}
          onpointerleave={cancelCardPress}
          onpointercancel={cancelCardPress}
        >
          {@render resBody(r)}
        </div>
        {#if r.num === readBoundaryNum && readBaseline < 1}
          <div class="read-bar" data-testid="read-boundary" aria-hidden="true">
            <span class="read-bar-label">前回ここまで</span>
          </div>
        {/if}
      {/each}
      {#if olderComplete && readBoundaryNum !== visibleRes[visibleRes.length - 1]?.num}
        <div class="read-bar" data-testid="thread-start" aria-hidden="true">
          <span class="read-bar-label">はじまり</span>
        </div>
      {/if}
    {/if}
  </div>

  <!-- Fixed footer: write (left) and refresh (right) icon buttons.
       Refresh is higher-frequency, so it takes the easier-to-reach right end. -->
  <div class="thread-footer">
    <!-- Icon buttons (36×36, 6px radius) — circular FABs are not used (DESIGN.md Shapes). -->
    <button
      class="btn icon-btn"
      aria-label="書き込む"
      onclick={() => {
        postModalOpen = true
        postError = null
      }}><Icon name="pencil" size="18" /></button
    >
    <button class="btn icon-btn" aria-label="更新" disabled={refreshing} onclick={triggerRefresh}
      ><Icon name="refresh-cw" size="18" /></button
    >
  </div>
</div>

<!-- Post (write) modal. -->
{#if postModalOpen}
  <Modal
    onclose={() => {
      postModalOpen = false
      postError = null
    }}
  >
    {#snippet header()}
      <div class="menu-title">書き込む</div>
    {/snippet}
    <div class="post-form">
      <div class="post-row">
        <label class="post-label">
          名前
          <input
            class="input"
            type="text"
            placeholder="（省略可）"
            bind:value={postName}
            disabled={postSubmitting}
          />
        </label>
        <label class="post-label">
          メール
          <input
            class="input"
            type="text"
            placeholder="sage"
            bind:value={postMail}
            disabled={postSubmitting}
          />
        </label>
      </div>
      <label class="post-label">
        本文
        <textarea
          class="post-textarea input"
          rows="6"
          placeholder="本文を入力"
          bind:value={postMessage}
          disabled={postSubmitting}></textarea>
      </label>
      {#if postError}
        <p class="post-error error" role="alert">{postError}</p>
      {/if}
      <button
        class="post-submit"
        disabled={postSubmitting || postMessage.trim() === ''}
        onclick={submitPost}
      >
        {postSubmitting ? '送信中…' : '書き込む'}
      </button>
    </div>
  </Modal>
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
        <div
          class="res id-list-res"
          role="group"
          aria-label="レス {r.num}"
          oncontextmenu={(e) => onCardContextMenu(e, r)}
          onpointerdown={(e) => onCardPointerDown(e, r)}
          onpointerup={cancelCardPress}
          onpointerleave={cancelCardPress}
          onpointercancel={cancelCardPress}
        >
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
        <div
          class="res id-list-res"
          role="group"
          aria-label="レス {r.num}"
          oncontextmenu={(e) => onCardContextMenu(e, r)}
          onpointerdown={(e) => onCardPointerDown(e, r)}
          onpointerup={cancelCardPress}
          onpointerleave={cancelCardPress}
          onpointercancel={cancelCardPress}
        >
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
      {#if !isWacchoiNgFor(wacchoiMenu.wacchoi, wacchoiMenu.date)}
        <button class="action" onclick={() => addNgWacchoi(wacchoiMenu.wacchoi, wacchoiMenu.date)}
          >NGﾜｯﾁｮｲに追加</button
        >
      {/if}
      <button class="action" onclick={() => copyWacchoi(wacchoiMenu.wacchoi)}>コピー</button>
      <button class="action" onclick={() => startWacchoiSearch(wacchoiMenu.wacchoi)}
        >取得済みスレから検索</button
      >
    </div>
  </Modal>
{/if}

<!-- NG post right-click / long-press menu. -->
{#if ngMenu != null}
  <Modal onclose={closeNgMenu}>
    {#snippet header()}
      <div class="menu-title">レス {ngMenu.resNum}（{ngMenu.label}）</div>
    {/snippet}
    <div class="menu" data-testid="ng-menu">
      <button class="action" onclick={removeNgReason}>{ngMenu.label}から削除</button>
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
      {#if !boardNgIds.has(idMenu)}
        <button class="action" onclick={() => addNg(idMenu)}>NGIDに追加</button>
      {/if}
      <button class="action" onclick={() => copyId(idMenu)}>コピー</button>
      <button class="action" onclick={() => startIdSearch(idMenu)}>取得済みスレから検索</button>
    </div>
  </Modal>
{/if}

<!-- Reply context menu modal: right-click on a res body opens this. -->
{#if replyMenuResNum != null}
  <Modal onclose={closeReplyMenu}>
    {#snippet header()}
      <div class="menu-title">レス {replyMenuResNum}</div>
    {/snippet}
    <div class="menu" data-testid="reply-menu">
      <button class="action" onclick={() => startReply(replyMenuResNum)}>返信する</button>
      <button class="action" onclick={() => copyBody(replyMenuResNum)}>本文をコピー</button>
      <button class="action" onclick={() => openNgWordForm(replyMenuResNum)}>NG Word に追加</button>
    </div>
  </Modal>
{/if}

<!-- NG Word registration modal (opened from the reply menu).
     The rule is saved for this board only and applies to every thread of it. -->
{#if ngWordForm != null}
  <Modal onclose={closeNgWordForm}>
    {#snippet header()}
      <div class="menu-title">NG Word に追加</div>
    {/snippet}
    <div class="ng-word-form" data-testid="ng-word-form">
      <!-- Segmented control: literal substring (default) or regular expression. -->
      <div class="segmented" role="group" aria-label="一致方法">
        <button
          class="segment"
          class:selected={ngWordForm.kind === 'text'}
          aria-pressed={ngWordForm.kind === 'text'}
          onclick={() => {
            ngWordForm.kind = 'text'
            ngWordForm.error = null
          }}>文字列</button
        >
        <button
          class="segment"
          class:selected={ngWordForm.kind === 'regex'}
          aria-pressed={ngWordForm.kind === 'regex'}
          onclick={() => {
            ngWordForm.kind = 'regex'
            ngWordForm.error = null
          }}>正規表現</button
        >
      </div>
      <textarea
        class="ng-word-textarea input"
        rows="5"
        aria-label="NG Word"
        bind:value={ngWordForm.pattern}
        disabled={ngWordForm.submitting}></textarea>
      <label class="ng-word-check">
        <input
          type="checkbox"
          bind:checked={ngWordForm.alsoId}
          disabled={ngWordForm.id == null || ngWordForm.submitting}
        />
        投稿者IDもNG
      </label>
      {#if ngWordForm.error}
        <p class="error" role="alert">{ngWordForm.error}</p>
      {/if}
      <div class="ng-word-actions">
        <button class="btn" onclick={closeNgWordForm} disabled={ngWordForm.submitting}
          >キャンセル</button
        >
        <button class="btn ng-word-submit" onclick={submitNgWord} disabled={ngWordForm.submitting}
          >追加</button
        >
      </div>
    </div>
  </Modal>
{/if}

<!-- Image context menu (thumbnail long-press or right-click). -->
{#if imageMenu != null}
  <Modal
    onclose={() => {
      imageMenu = null
    }}
  >
    {#snippet header()}<div class="menu-title">画像</div>{/snippet}
    <div class="menu" data-testid="image-menu">
      <button class="action" onclick={() => toggleMosaic(imageMenu.url)}>
        {imageMenu.mosaic ? 'モザイクを解除' : 'モザイクをかける'}
      </button>
      <button
        class="action"
        onclick={() => {
          copyText(imageMenu.url)
          imageMenu = null
        }}>URL をコピー</button
      >
    </div>
  </Modal>
{/if}

<!-- Full-screen image viewer. -->
{#if imageViewerState != null}
  <ImageViewer
    images={imageViewerState.images}
    initialIndex={imageViewerState.initialIndex}
    {mosaicUrls}
    onclose={() => {
      imageViewerState = null
    }}
    onImageMenu={(item) => {
      imageViewerState = null
      imageMenu = { url: item.url, mosaic: mosaicUrls.has(item.url) }
    }}
  />
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
  /* Thread view: fills the full height of its container (detail-pane on PC, viewport on phone).
     Flex column so header / body / pull-panel / footer stack vertically. */
  .thread-view {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* Sticky title header. .title's nearest scroll ancestor is .thread-body
     (overflow-y:auto), which fills .detail-pane. The body's top edge sits
     directly below the sticky NavBar, so top:0 pins the title flush to
     that edge on both phone and PC. */
  .title {
    position: sticky;
    top: 0;
    z-index: 5;
    flex-shrink: 0;
    margin: 0;
    padding: 8px 0;
    font-size: 17px;
    font-weight: 600;
    line-height: 1.3;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Scrollable body: fills all remaining space between the header and the footer. */
  .thread-body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 4px 0;
  }

  /* Read boundary / end bar: a hr-like horizontal rule with a centred muted label.
     Deliberately unlike a .res card (no surface bg, no radius, no left border) so it
     never reads as a post. The label sits centred over the line. */
  .read-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .read-bar::before,
  .read-bar::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--border);
  }
  .read-bar-label {
    flex-shrink: 0;
  }

  /* Res card: 8px radius (list-row scale), surface-raised on surface. */
  .res {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px;
    margin-bottom: 4px;
    /* Prefer the custom card menu over the native long-press callout. */
    -webkit-touch-callout: none;
  }
  /* Unread: orange left border (--unread). Not --danger because unread is not an error. */
  .res.unread {
    border-left: 3px solid var(--unread);
  }
  /* Own post: pink left border (--own). Mutually exclusive with .unread via JS (class:unread={...&&!r.own}). */
  .res.own {
    border-left: 3px solid var(--own);
  }
  .num {
    font-weight: bold;
    color: var(--name);
  }
  .name {
    color: var(--name);
    margin-left: 4px;
  }
  .date {
    font-size: 12px;
    color: var(--muted);
    margin-left: 4px;
  }
  .body {
    margin-top: 4px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  :global(.anchor) {
    color: var(--link);
    cursor: pointer;
    text-decoration: underline;
  }
  /* External URL links inside res bodies (output by linkify.js).
     Share --link with .anchor so all clickable links share the same hue. */
  :global(.body a),
  :global(.body a:visited) {
    color: var(--link);
    text-decoration: underline;
  }
  :global(.body a:hover) {
    text-decoration: none;
  }
  /* Inline wacchoi badge: inherits colour from .name so that per-res colour
     classes (id-l2..l5) applied to .name propagate without extra wrappers.
     Clickable-badge affordance (cursor) is shared with .resid below. */
  :global(.wacchoi-badge) {
    color: inherit;
  }
  .backrefs {
    margin-top: 4px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 12px;
  }
  /* Anchor tree: depth-driven indentation via inline margin-left (set per node).
     A left border on each indented node gives the visual tree guide. */
  .anchor-node {
    /* padding-left leaves room for the border without shifting text too much */
    padding-left: 8px;
    border-left: 2px solid var(--border);
  }
  /* Tighter vertical spacing inside the tree for scannability. */
  .anchor-node .res {
    margin-bottom: 4px;
  }
  .anchor-node .res.missing {
    color: var(--muted);
    font-size: 14px;
  }
  /* Highlight the pivot res (the clicked N) in the anchor tree.
     border-left here overrides the node-level border and gives a coloured accent;
     indentation is unaffected (it comes from margin-left on .anchor-node). */
  .anchor-node .res.anchor-self {
    border-left: 3px solid var(--accent);
    background: var(--accent-subtle);
  }
  .refresh-error {
    margin: 8px 0;
    font-size: 14px;
  }
  /* ID/wacchoi badge: same font-size as surrounding .date text.
     Gap from the preceding element comes from an &nbsp; placed inside
     each {#if} block (just before the span), so it only renders when the badge is shown. */
  .id-badge {
    font-size: 12px;
  }
  /* id-l1: single-occurrence ID — shown as muted text (clickable but no colour accent). */
  .id-l1 {
    color: var(--muted);
  }
  .id-l2 {
    color: var(--id-l2);
  }
  .id-l3 {
    color: var(--id-l3);
  }
  .id-l4 {
    color: var(--id-l4);
  }
  .id-l5 {
    color: var(--id-l5);
    font-weight: bold;
  }

  /* Clickable-badge affordance, shared by the ID badge and the wacchoi badge.
     Selection suppression is handled app-wide (DESIGN.md Touch text-selection policy). */
  .resid,
  :global(.wacchoi-badge) {
    cursor: pointer;
  }

  /* NG disclosure: terse, muted and keyboard-focusable; its body is opt-in. */
  .ng-toggle {
    display: block;
    width: fit-content;
    max-width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    color: var(--muted);
    background: transparent;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ng {
    color: var(--muted);
    text-decoration: line-through;
    font-size: 12px;
    line-height: 1.5;
  }
  .ng-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* ID action menu (same layout as FavoritesList menu). */
  .menu {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 16rem;
    max-width: 100%;
  }
  .menu-title {
    font-weight: 600;
    word-break: break-all;
    font-size: 14px;
  }
  /* .action styling comes from the shared .menu recipes in App.svelte. */

  /* ID list (same-ID reses in the current thread) + ID search result modal content.
     width + max-width:100% (matches .menu) so they fill the modal's effective
     content width on phones and stay a comfortable width on desktop. A
     viewport-based min-width would overflow because it ignores the scrim +
     modal padding (16px x2 each). */
  .id-list,
  .search-result {
    width: 32rem;
    max-width: 100%;
  }
  .id-list-res {
    font-size: 14px;
  }
  .search-thread-title {
    font-size: 14px;
    color: var(--accent);
    margin: 12px 0 4px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 4px;
    /* Long titles without break opportunities must wrap, not widen the modal. */
    word-break: break-word;
  }
  .search-res {
    font-size: 14px;
  }
  .search-empty {
    color: var(--muted);
  }

  /* Fixed footer: stays at the bottom of .thread-view.
     space-between splits the two icon buttons to the left/right ends. */
  .thread-footer {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }

  /* Post form inside the modal. */
  .post-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 28rem;
    max-width: 100%;
  }
  /* Name + mail fields side by side to save vertical space. */
  .post-row {
    display: flex;
    gap: 8px;
  }
  /* Field labels: caption-size muted text above the field (DESIGN.md Inputs). */
  .post-label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  /* Inside .post-row each label stretches equally. */
  .post-row .post-label {
    flex: 1;
    min-width: 0;
  }
  /* Fields use the shared .input recipe (App.svelte). */
  .post-textarea {
    resize: vertical;
  }
  .post-error {
    margin: 0;
    font-size: 14px;
  }
  /* Primary button: accent bg — the single primary action of this screen. */
  .post-submit {
    padding: 8px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--surface-raised);
    font-size: 15px;
    cursor: pointer;
    font-weight: 600;
  }
  .post-submit:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* NG Word form: same column rhythm as the post form. */
  .ng-word-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 28rem;
    max-width: 100%;
  }
  /* Segmented control: two equal segments sharing one bordered track, the divider
     being the second segment's left border so no double hairline appears. The
     selected segment is filled with the subtle accent tint; the control is chrome,
     so it never uses a data color. */
  .segmented {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .segment {
    flex: 1;
    padding: 8px;
    border: none;
    background: var(--surface-raised);
    color: var(--muted);
    font-size: 15px;
    font-family: inherit;
    font-weight: 500;
    line-height: 1.2;
    cursor: pointer;
  }
  .segment + .segment {
    border-left: 1px solid var(--border);
  }
  .segment.selected {
    background: var(--accent-subtle);
    color: var(--on-surface);
  }
  .ng-word-textarea {
    resize: vertical;
  }
  /* Checkbox row: body-size label next to the box (not a caption field label). */
  .ng-word-check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
  }
  .ng-word-actions {
    display: flex;
    gap: 8px;
  }
  .ng-word-actions .btn {
    flex: 1;
  }
  /* Primary button of this modal: accent fill, matching the post modal's submit. */
  .ng-word-submit {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--surface-raised);
    font-weight: 600;
  }
  .ng-word-submit:hover:not(:disabled) {
    background: var(--accent);
  }
  .ng-word-form .error {
    margin: 0;
    font-size: 14px;
  }

  /* Thumbnail strip: images below the body text. */
  .thumb-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }

  /* Thumbnail button: borderless, resets button defaults.
     position:relative anchors the .thumb-error overlay. */
  .thumb-btn {
    position: relative;
    padding: 0;
    border: none;
    background: none;
    cursor: pointer;
    border-radius: 6px;
    overflow: hidden;
    flex-shrink: 0;
  }

  /* Thumbnail image: fixed 96×96 px square, cropped to cover. */
  .thumb {
    display: block;
    width: 96px;
    height: 96px;
    object-fit: cover;
    border-radius: 6px;
    background: var(--border);
  }

  /* Mosaic: strong blur applied over the thumbnail. */
  .thumb-btn img.thumb-mosaic {
    filter: blur(20px);
  }

  /* Failed image load: muted placeholder background... */
  .thumb.thumb-missing {
    background: var(--border);
  }
  /* ...with an SVG cross overlay (shown only next to a failed .thumb).
     :global() because .thumb-missing is added at runtime via classList. */
  .thumb-error {
    display: none;
  }
  .thumb:global(.thumb-missing) + .thumb-error {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    pointer-events: none;
  }
</style>
