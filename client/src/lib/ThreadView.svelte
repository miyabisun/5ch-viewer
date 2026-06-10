<script>
  import { untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'
  import { formatName } from './name.js'

  let { fav, onback } = $props()

  let data = $state(null)
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
    } catch {
      // Refresh is best-effort; fall back to whatever the dat endpoint returns.
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
      if (node) node.scrollIntoView({ block: 'end' })
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
  function backSwipe(node) {
    let startX, startY, locked, horizontal
    function onStart(e) {
      const t = e.touches[0]
      startX = t.clientX
      startY = t.clientY
      locked = false
      horizontal = false
    }
    function onMove(e) {
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
      if (!locked || !horizontal) return
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
  <div class="modal-bg" role="presentation" onclick={closeAnchor}>
    <div class="modal" role="presentation" onclick={(e) => e.stopPropagation()}>
      <div class="modal-bar">
        {#if anchorStack.length > 1}
          <button onclick={popAnchor}>← 戻る</button>
        {/if}
        <button class="close" onclick={closeAnchor}>閉じる</button>
      </div>
      {#if currentAnchor.missing}
        <p>レス {currentAnchor.num} は未取得です</p>
      {:else}
        <!-- Anchors inside the modal are also followable (recursively pushed onto the stack). -->
        <div class="res">
          {@render resBody(currentAnchor)}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Sticky title header. Sits below the global NavBar (which is sticky at top:0
     and ~2.8rem tall: theme toggle 2rem + 0.4rem padding x2). */
  .title {
    position: sticky;
    top: 2.8rem;
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
  .modal-bar {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .modal-bar .close {
    margin-left: auto;
  }
  .modal-bg {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
  }
  .modal {
    background: var(--card-bg);
    border-radius: 8px;
    padding: 1rem;
    max-width: 100%;
    max-height: 80%;
    overflow: auto;
  }
</style>
