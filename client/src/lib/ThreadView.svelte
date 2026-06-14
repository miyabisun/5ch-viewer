<script>
  import { untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'
  import { formatName } from './name.js'
  import { stripId, buildIdStats } from './id.js'
  import { buildWacchoiStats, wacchoiEnabled } from './wacchoi.js'
  import Modal from './Modal.svelte'

  let { fav, onback, ngIds = new Set(), onngchange = () => {} } = $props()

  let data = $state(null)
  // Surfaced when the auto-refresh (reload) fails. The stored dat is still shown
  // below, so this is a non-blocking notice rather than a hard error.
  let refreshError = $state(null)
  // Read position (max res number that has passed through the viewport). Initialized from the saved read_res (only on first mount).
  let maxRead = $state(untrack(() => fav.read_res))
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
  let timer
  $effect(() => {
    const n = maxRead
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

  // Pre-computed child list for the current anchor tree.
  // Built once from (anchorRoot, data) with no rendering side-effects.
  // BFS over the forward-anchor graph: each res number enters the visited set
  // when it is *enqueued* (not when dequeued), so DAG nodes reachable via
  // multiple paths are only expanded once and appear exactly once in the tree.
  // Maps res number -> array of child res numbers to display under it.
  const anchorChildren = $derived.by(() => {
    const map = new Map()
    if (anchorRoot == null || !data?.res) return map
    const visited = new Set([anchorRoot])
    const queue = [anchorRoot]
    while (queue.length > 0) {
      const num = queue.shift()
      const r = resOf(num)
      if (r.missing) {
        map.set(num, [])
        continue
      }
      const children = []
      // matchAll clones the regex internally, so sharing ANCHOR_RE here does not
      // corrupt the lastIndex used by the exec-based backrefs loop.
      for (const m of r.body.matchAll(ANCHOR_RE)) {
        const n = Number(m[1])
        // Skip: self-reference or already enqueued/visited (prevents cycles and DAG duplication).
        if (n === r.num || visited.has(n)) continue
        visited.add(n)
        children.push(n)
        queue.push(n)
      }
      map.set(num, children)
    }
    return map
  })

  // Body click (shared by list and modal). Follow the anchor when tapped.
  function onBodyClick(e) {
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
      // Ignore multi-touch and touches that begin on an interactive anchor.
      ignore = e.touches.length > 1 || !!e.target.closest('.anchor')
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
      if (anchorRoot != null || idListId != null || idMenu != null || wacchoiListKey != null) return
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
    try {
      await navigator.clipboard.writeText('ID:' + id)
    } catch {
      /* clipboard may be unavailable; fail silently */
    }
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
  <span class="num">{r.num}</span>
  <span class="name">{formatName(r.name)}</span>
  <span class="date">
    {stripId(r.date)}{#if resolvedId}
      <!-- ID span: always shown when an ID exists (total>=2 only controls colour).
           Right-click (PC) or long-press (touch) opens the ID context menu.
           For total>=2 the span is coloured and shows order/total counts. -->
      {@const colorCls = stats && stats.total >= 2 ? `id-${stats.colorLevel}` : 'id-l1'}
      {@const label = stats && stats.total >= 2
        ? `ID:${resolvedId} (${stats.order}/${stats.total})`
        : `ID:${resolvedId}`}
      <span
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
    {/if}{#if wStats && wStats.total >= 2}
      <!-- Wacchoi badge: wacchoiStats is empty unless the thread has wacchoi, so a
           present wStats already implies enabled. Shown only when this res has 2+
           posts from the same wacchoi (total=1 is intentionally hidden per spec). -->
      {@const wLabel = `ﾜｯﾁｮｲ:${wStats.wacchoi} (${wStats.order}/${wStats.total})`}
      <span
        class="id-badge id-{wStats.colorLevel} resid"
        role="button"
        tabindex="0"
        data-wacchoi={wStats.wacchoi}
        onclick={() => openWacchoiList(wStats.wacchoi)}
        onkeydown={(e) => e.key === 'Enter' && openWacchoiList(wStats.wacchoi)}
      >{wLabel}</span>
    {/if}
  </span>
{/snippet}

<!-- resHead + body snippet combined (used in main list). -->
{#snippet resHeadAndBody(r)}
  {@const ngd = r.id ? ngIds.has(r.id) : false}
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

<!-- Recursive anchor tree node.
     Reads pre-computed anchorChildren (a derived Map) — no rendering side-effects.
     Each res appears at most once in the tree (DAG/cycle-safe by construction). -->
{#snippet anchorNode(num)}
  {@const r = resOf(num)}
  {@const children = anchorChildren.get(num) ?? []}
  <div class="anchor-node">
    {#if r.missing}
      <div class="res missing">レス {num} は未取得です</div>
    {:else}
      <!-- No back-references inside the tree, to reduce noise.
           Uses resHeadAndBody so NG posts are hidden even inside the anchor tree. -->
      <div class="res">
        {@render resHeadAndBody(r)}
      </div>
      {#each children as child (child)}
        {@render anchorNode(child)}
      {/each}
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
  <div class="thread-body" use:backSwipe>
    {#each data.res as r (r.num)}
      <div class="res" use:track={r.num} class:unread={r.num > fav.read_res}>
        {@render resBody(r)}
      </div>
    {/each}
  </div>
{/if}

{#if anchorRoot != null}
  <Modal onclose={closeAnchor}>
    <!-- Clicking >>N inside the tree replaces the root (no stacking). -->
    <div role="presentation" onclick={onBodyClick}>
      {@render anchorNode(anchorRoot)}
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

<!-- ID search modal: shows a loading notice while fetching, then the result list. -->
{#if idSearchLoading || idSearchResult != null}
  <Modal onclose={closeIdSearch}>
    {#snippet header()}
      <div class="menu-title">
        {#if idSearchLoading}
          ID:{idSearchTarget} を検索中…
        {:else}
          ID:{idSearchTarget} の検索結果（{idSearchHits}件）
        {/if}
      </div>
    {/snippet}
    {#if !idSearchLoading}
      <div class="search-result" data-testid="id-search-result">
        {#if idSearchResult.length === 0}
          <p class="search-empty">該当なし</p>
        {:else}
          {#each idSearchResult as thread (thread.thread_id)}
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
  .backrefs {
    margin-top: 0.3rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    font-size: 0.8rem;
  }
  /* Indent each level of the anchor tree with a visible left border. */
  .anchor-node > .anchor-node {
    margin-left: 0.6rem;
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
  .refresh-error {
    margin: 0.4rem 0;
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
    color: var(--danger);
    background: var(--error-bg);
    border-radius: 4px;
  }
  /* ID badge: same size as the surrounding .date text. */
  .id-badge {
    font-size: 0.75rem;
  }
  /* id-l1: single-occurrence ID — shown as muted text (clickable but no colour accent). */
  .id-l1 { color: var(--muted); }
  .id-l2 { color: var(--id-l2); }
  .id-l3 { color: var(--id-l3); }
  .id-l4 { color: var(--id-l4); }
  .id-l5 { color: var(--id-l5); font-weight: bold; }

  /* Clickable ID badge affordance. */
  .resid {
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
</style>
