<script>
  import { untrack } from 'svelte'
  import { api, beaconProgress } from './api.js'

  let { fav, onback } = $props()

  let data = $state(null)
  let loading = $state(false)
  // Read position (max res number that has passed through the viewport). Initialized from the saved read_res (only on first mount).
  let maxRead = $state(untrack(() => fav.read_res))

  async function load() {
    data = await api.getDat(fav.server, fav.board, fav.thread_id)
    if (data.read_res > maxRead) maxRead = data.read_res
  }

  async function reload() {
    loading = true
    try {
      await api.reload(fav.server, fav.board, fav.thread_id)
      await load()
    } catch (e) {
      alert(e.message)
    } finally {
      loading = false
    }
  }

  // Initial load.
  $effect(() => {
    load()
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
</script>

<div class="bar">
  <button onclick={onback}>← 戻る</button>
  <button onclick={reload} disabled={loading}>{loading ? '更新中…' : '↻ 更新'}</button>
</div>

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
  <span class="name">{r.name}</span>
  <span class="date">{r.date}</span>
  {@render body(r.body)}
  {@render refs(r.num)}
{/snippet}

{#if data}
  <h1>{data.title || fav.title}</h1>
  {#each data.res as r (r.num)}
    <div class="res" use:track={r.num} class:unread={r.num > fav.read_res}>
      {@render resBody(r)}
    </div>
  {/each}
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
  .bar {
    position: sticky;
    top: 0;
    background: #fafafa;
    display: flex;
    gap: 0.5rem;
    padding: 0.5rem 0;
  }
  h1 {
    font-size: 1.1rem;
  }
  .res {
    background: #fff;
    border: 1px solid #eee;
    border-radius: 6px;
    padding: 0.5rem;
    margin-bottom: 0.3rem;
  }
  .res.unread {
    border-left: 3px solid #c00;
  }
  .num {
    font-weight: bold;
    color: #060;
  }
  .name {
    color: #060;
    margin-left: 0.3rem;
  }
  .date {
    font-size: 0.75rem;
    color: #999;
    margin-left: 0.3rem;
  }
  .body {
    margin-top: 0.3rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  :global(.anchor) {
    color: #1a6;
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
    background: #fff;
    border-radius: 8px;
    padding: 1rem;
    max-width: 100%;
    max-height: 80%;
    overflow: auto;
  }
</style>
