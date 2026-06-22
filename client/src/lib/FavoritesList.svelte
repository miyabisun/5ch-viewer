<script>
  import { api } from './api.js'
  import ThreadRow from './ThreadRow.svelte'
  import ThreadMenu from './ThreadMenu.svelte'
  import { topPullRefresh, PULL_THRESHOLD_PX } from './topPullRefresh.js'

  let { favorites, onopen, onchange, onrefresh = () => {} } = $props()

  const collator = new Intl.Collator('ja', { numeric: true })

  // Group by rating; groups sorted by rating desc, items within a group by natural title order.
  let groups = $derived.by(() => {
    const map = new Map()
    for (const f of favorites) {
      if (!map.has(f.rating)) map.set(f.rating, [])
      map.get(f.rating).push(f)
    }
    const entries = [...map.entries()].sort((a, b) => b[0] - a[0])
    for (const [, arr] of entries) {
      arr.sort((a, b) => collator.compare(a.title, b.title))
    }
    return entries
  })

  // --- Refresh state ---
  let refreshing = $state(false)
  let pullPx = $state(0)
  // Above-threshold is purely derived from the current pull distance.
  let aboveThreshold = $derived(pullPx >= PULL_THRESHOLD_PX)

  // Touch device detection: pull-to-refresh is touch-only.
  const isTouch =
    typeof window !== 'undefined' &&
    matchMedia('(hover: none) and (pointer: coarse)').matches

  async function triggerRefresh() {
    if (refreshing) return
    refreshing = true
    try {
      await onrefresh()
    } finally {
      refreshing = false
    }
  }

  // --- Action menu (right-click PC / long-press mobile) ---
  // A single modal-style menu avoids mis-taps on phones (the previous inline
  // select + × buttons were easy to hit by accident — see docs/discussions.md).
  let menu = $state(null) // the favorite the menu acts on, or null

  function openMenu(f) {
    menu = f
  }
  function closeMenu() {
    menu = null
  }

  async function setRating(f, rating) {
    await api.setRating(f.server, f.board, f.thread_id, rating)
    closeMenu()
    onchange()
  }

  async function archive(f) {
    await api.setArchived(f.server, f.board, f.thread_id, true)
    closeMenu()
    onchange()
  }
</script>

<div class="favorites-view">
  <!-- Top pull-to-refresh panel: height driven by pullPx (0 = hidden). -->
  <div
    class="pull-refresh-panel top"
    class:above-threshold={aboveThreshold}
    style="height: {refreshing ? '4rem' : pullPx > 0 ? pullPx + 'px' : '0'}"
    data-testid="pull-refresh-top"
  >
    {#if refreshing}
      <span class="pull-refresh-spinner"></span>
      <span>更新中…</span>
    {:else if aboveThreshold}
      <span>↑ 離して更新</span>
    {:else}
      <span>↓ 引いて更新</span>
    {/if}
  </div>

  <!-- Main scrollable content -->
  <div
    class="favorites-body"
    use:topPullRefresh={() => ({
      enabled: isTouch && !refreshing,
      isBlocked: () => menu != null,
      onRefresh: triggerRefresh,
      onDrag: (px) => (pullPx = px),
    })}
  >
    {#each groups as [rating, threads] (rating)}
      <h2>{rating > 0 ? '★'.repeat(rating) : '☆ 未分類'}</h2>
      {#each threads as f (f.server + f.board + f.thread_id)}
        <ThreadRow
          thread={f}
          {onopen}
          onmenu={openMenu}
          extraClass="rate-{f.rating}"
          dataRating={f.rating}
          style="--row-color: var(--rate-{f.rating})"
        />
      {/each}
    {/each}

    {#if menu}
      <ThreadMenu {menu} onclose={closeMenu} onremoved={onchange} onarchive={archive}>
        {#snippet actions(f)}
          <div class="section-label">お気に入りレベル</div>
          <!--
            Star rating: 6 characters total — leftmost ☆ (r=0) always clears to rating 0;
            r=1..5 are lit (★) when f.rating >= r, otherwise ☆.
            Selection is shown by COLOR, not underline.
          -->
          <div class="stars">
            <!-- r=0: always ☆, clicking sets rating to 0 (remove from favorites level). -->
            <a
              href="#rate-0"
              class="star off"
              data-rating={0}
              aria-label="お気に入り解除"
              onclick={(e) => {
                e.preventDefault()
                setRating(f, 0)
              }}
            >☆</a>
            {#each [1, 2, 3, 4, 5] as r}
              {@const lit = f.rating >= r}
              <a
                href="#rate-{r}"
                class="star"
                class:on={lit}
                class:off={!lit}
                data-rating={r}
                aria-label="レベル {r}"
                onclick={(e) => {
                  e.preventDefault()
                  setRating(f, r)
                }}
              >{lit ? '★' : '☆'}</a>
            {/each}
          </div>
        {/snippet}
      </ThreadMenu>
    {/if}
  </div>

  <!-- Sticky footer with refresh button -->
  <div class="favorites-footer">
    <button
      class="refresh-btn"
      data-testid="favorites-refresh-btn"
      disabled={refreshing}
      onclick={triggerRefresh}
      aria-label="更新"
    >
      🔄 更新
    </button>
  </div>
</div>

<style>
  .favorites-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  /* Top pull-to-refresh panel: grows from top as user drags down. */
  .pull-refresh-panel.top {
    flex-shrink: 0;
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    font-size: 0.95rem;
    background: var(--card-bg);
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    user-select: none;
    pointer-events: none;
    transition: height 0.05s linear;
  }
  .pull-refresh-panel.top.above-threshold {
    color: var(--accent);
    font-weight: 600;
  }
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

  .favorites-body {
    flex: 1;
    min-height: 0;
  }

  h2 {
    font-size: 1rem;
    color: var(--accent);
    margin: 1rem 0 0.3rem;
  }

  /* Star rating: color, not underline, conveys selection. */
  .stars {
    display: flex;
    justify-content: center;
    gap: 0.2rem;
    font-size: 1.6rem;
    line-height: 1;
  }
  .star {
    text-decoration: none;
    cursor: pointer;
  }
  .star.on {
    color: var(--accent);
  }
  .star.off {
    color: var(--rate-0);
  }

  /* Sticky footer: mirrors .thread-footer from ThreadView for visual consistency. */
  .favorites-footer {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 0.4rem 0.6rem;
    background: var(--bg);
    border-top: 1px solid var(--border);
    position: sticky;
    bottom: 0;
    z-index: 5;
  }
  .refresh-btn {
    border: 1px solid var(--border);
    background: var(--card-bg);
    color: var(--fg);
    border-radius: 6px;
    padding: 0.4rem 0.8rem;
    font-size: 0.95rem;
    cursor: pointer;
    line-height: 1.4;
  }
  .refresh-btn:hover:not(:disabled) {
    background: var(--border);
  }
  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
