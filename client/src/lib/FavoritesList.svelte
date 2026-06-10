<script>
  import { api } from './api.js'

  let { favorites, onopen, onchange } = $props()

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

  // 5ch thread URL (docs/5ch-spec.md): https://{server}.5ch.io/test/read.cgi/{board}/{thread_id}/
  function threadUrl(f) {
    return `https://${f.server}.5ch.io/test/read.cgi/${f.board}/${f.thread_id}/`
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

  // Long-press detection for touch devices.
  let pressTimer
  let longPressed = false
  function onPointerDown(e, f) {
    if (e.pointerType !== 'touch') return
    longPressed = false
    pressTimer = setTimeout(() => {
      longPressed = true
      openMenu(f)
    }, 500)
  }
  function cancelPress() {
    clearTimeout(pressTimer)
  }

  function onItemClick(f) {
    // Swallow the click that ends a long-press so the thread does not open.
    if (longPressed) {
      longPressed = false
      return
    }
    onopen(f)
  }

  async function setRating(f, rating) {
    await api.setRating(f.server, f.board, f.thread_id, rating)
    closeMenu()
    onchange()
  }

  async function remove(f) {
    if (!confirm(`削除しますか？\n${f.title}`)) return
    await api.removeFavorite(f.server, f.board, f.thread_id)
    closeMenu()
    onchange()
  }

  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      /* clipboard may be unavailable; fail silently */
    }
    closeMenu()
  }
</script>

{#each groups as [rating, threads] (rating)}
  <h2>{rating > 0 ? '★'.repeat(rating) : '☆ 未分類'}</h2>
  {#each threads as f (f.server + f.board + f.thread_id)}
    {@const unread = f.res_count - f.read_res}
    <div
      class="thread rate-{f.rating}"
      class:dead={f.status === 'dead'}
      data-rating={f.rating}
      role="button"
      tabindex="0"
      oncontextmenu={(e) => {
        e.preventDefault()
        openMenu(f)
      }}
      onpointerdown={(e) => onPointerDown(e, f)}
      onpointerup={cancelPress}
      onpointerleave={cancelPress}
      onpointercancel={cancelPress}
      onclick={() => onItemClick(f)}
      onkeydown={(e) => e.key === 'Enter' && onopen(f)}
    >
      <div class="info">
        <div class="title">{f.title || '(未取得 — 開いて更新)'}</div>
        <div class="sub">{f.board_name} {f.res_count}</div>
      </div>
      {#if unread > 0}
        <span class="unread">[{unread}]</span>
      {/if}
    </div>
  {/each}
{/each}

{#if menu}
  <div class="menu-bg" role="presentation" onclick={closeMenu}>
    <div class="menu" role="presentation" onclick={(e) => e.stopPropagation()}>
      <div class="menu-title">{menu.title || '(未取得)'}</div>

      <div class="section-label">お気に入りレベル</div>
      <div class="ratings">
        {#each [0, 1, 2, 3, 4, 5] as r}
          <button
            class="rate-btn rate-{r}"
            class:current={menu.rating === r}
            data-rating={r}
            onclick={() => setRating(menu, r)}
          >
            {r > 0 ? '★'.repeat(r) : '☆'}
          </button>
        {/each}
      </div>

      <div class="section-label">コピー</div>
      <button class="action" onclick={() => copy(menu.title)}>タイトルをコピー</button>
      <button class="action" onclick={() => copy(threadUrl(menu))}>URL をコピー</button>
      <button class="action" onclick={() => copy(`${menu.title}\n${threadUrl(menu)}`)}>
        タイトル+URL をコピー
      </button>

      <button class="action danger" onclick={() => remove(menu)}>削除</button>
      <button class="action close" onclick={closeMenu}>閉じる</button>
    </div>
  </div>
{/if}

<style>
  h2 {
    font-size: 1rem;
    color: var(--accent);
    margin: 1rem 0 0.3rem;
  }
  .thread {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    padding-left: 0.6rem;
    background: var(--card-bg);
    border: 1px solid var(--border);
    /* The rating color bar lives on the left edge. */
    border-left: 4px solid var(--rate-color);
    border-radius: 6px;
    margin-bottom: 0.3rem;
    cursor: pointer;
    /* Disable the browser's long-press text selection / callout on touch. */
    -webkit-touch-callout: none;
    user-select: none;
  }
  .thread.dead {
    opacity: 0.5;
  }
  /* Rating -> bar color. */
  .rate-0 {
    --rate-color: var(--rate-0);
  }
  .rate-1 {
    --rate-color: var(--rate-1);
  }
  .rate-2 {
    --rate-color: var(--rate-2);
  }
  .rate-3 {
    --rate-color: var(--rate-3);
  }
  .rate-4 {
    --rate-color: var(--rate-4);
  }
  .rate-5 {
    --rate-color: var(--rate-5);
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .sub {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .unread {
    color: var(--danger);
    font-weight: bold;
  }

  .menu-bg {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 10;
  }
  .menu {
    background: var(--card-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
    width: 100%;
    max-width: 360px;
    max-height: 80%;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .menu-title {
    font-weight: 600;
    word-break: break-word;
    margin-bottom: 0.3rem;
  }
  .section-label {
    font-size: 0.75rem;
    color: var(--muted);
    margin-top: 0.4rem;
  }
  .ratings {
    display: flex;
    gap: 0.3rem;
  }
  .rate-btn {
    flex: 1;
    padding: 0.5rem 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    font-size: 0.75rem;
    border-bottom: 3px solid var(--rate-color);
  }
  .rate-btn.current {
    outline: 2px solid var(--accent);
  }
  .action {
    padding: 0.7rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    text-align: left;
    font-size: 0.95rem;
  }
  .action.danger {
    color: var(--danger);
    margin-top: 0.4rem;
  }
  .action.close {
    text-align: center;
  }
</style>
