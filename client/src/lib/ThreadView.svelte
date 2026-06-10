<script>
  import { untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'
  import { formatName } from './name.js'
  import Modal from './Modal.svelte'

  let { fav, onback } = $props()

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

  // Look up a res by number (missing if not found).
  function resOf(num) {
    return data?.res.find((r) => r.num === num) ?? { num, missing: true }
  }

  // The modal is stack-based; the top is the currently displayed res.
  // Tapping either an anchor target or source pushes; "back" pops.
  let anchorStack = $state([])
  const currentAnchor = $derived(anchorStack[anchorStack.length - 1] ?? null)

  function openAnchor(num) {
    anchorStack = [...anchorStack, resOf(num)]
  }
  function popAnchor() {
    anchorStack = anchorStack.slice(0, -1)
  }
  function closeAnchor() {
    anchorStack = []
  }

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
      // A modal owns the interaction while open; don't leave the thread.
      if (anchorStack.length > 0) return
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

<!-- Res body (shared by list and modal). Anchors in body and back-references are followable. -->
{#snippet resBody(r)}
  <span class="num">{r.num}</span>
  <span class="name">{formatName(r.name)}</span>
  <span class="date">{r.date}</span>
  {@render body(r.body)}
  {@render refs(r.num)}
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

{#if currentAnchor}
  <Modal onclose={closeAnchor}>
    {#snippet header()}
      {#if anchorStack.length > 1}
        <button class="back" onclick={popAnchor}>← 戻る</button>
      {/if}
    {/snippet}
    {#if currentAnchor.missing}
      <p>レス {currentAnchor.num} は未取得です</p>
    {:else}
      <!-- Anchors inside the modal are also followable (recursively pushed onto the stack). -->
      <div class="res">
        {@render resBody(currentAnchor)}
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
  .back {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.2rem 0.5rem;
    color: var(--fg);
    cursor: pointer;
  }
  .refresh-error {
    margin: 0.4rem 0;
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
    color: var(--danger);
    background: var(--error-bg);
    border-radius: 4px;
  }
</style>
