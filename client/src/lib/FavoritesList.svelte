<script>
  import { api } from './api.js'
  import Modal from './Modal.svelte'

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
        <span class="unread">{unread}</span>
      {/if}
    </div>
  {/each}
{/each}

{#if menu}
  <Modal onclose={closeMenu}>
    {#snippet header()}
      {menu.title || '(未取得)'}
    {/snippet}

    <!-- .menu is the action body and the stable E2E hook for the open menu. -->
    <div class="menu">
      <div class="menu-url">{threadUrl(menu)}</div>

      <div class="section-label">お気に入りレベル</div>
      <!--
        Star rating: 6 characters total — leftmost ☆ (r=0) always clears to rating 0;
        r=1..5 are lit (★) when menu.rating >= r, otherwise ☆.
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
            setRating(menu, 0)
          }}
        >☆</a>
        {#each [1, 2, 3, 4, 5] as r}
          {@const lit = menu.rating >= r}
          <a
            href="#rate-{r}"
            class="star"
            class:on={lit}
            class:off={!lit}
            data-rating={r}
            aria-label="レベル {r}"
            onclick={(e) => {
              e.preventDefault()
              setRating(menu, r)
            }}
          >{lit ? '★' : '☆'}</a>
        {/each}
      </div>

      <div class="section-label">コピー</div>
      <button class="action" onclick={() => copy(menu.title)}>タイトルをコピー</button>
      <button class="action" onclick={() => copy(threadUrl(menu))}>URL をコピー</button>
      <button class="action" onclick={() => copy(`${menu.title}\n${threadUrl(menu)}`)}>
        タイトル+URL をコピー
      </button>

      <button class="action danger" onclick={() => remove(menu)}>削除</button>
    </div>
  </Modal>
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
  /* Unread badge: dark-red rounded pill, white text. Hidden when 0 (not rendered). */
  .unread {
    flex: none;
    background: var(--badge-bg);
    color: var(--badge-fg);
    font-size: 0.75rem;
    font-weight: bold;
    line-height: 1;
    padding: 0.2rem 0.45rem;
    border-radius: 999px;
  }

  .menu {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    width: 18rem;
    max-width: 100%;
  }
  .menu-url {
    font-size: 0.75rem;
    color: var(--muted);
    word-break: break-all;
    margin-top: 0.15rem;
  }
  .section-label {
    font-size: 0.75rem;
    color: var(--muted);
    margin-top: 0.4rem;
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
  .action {
    width: 100%;
    padding: 0.7rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    text-align: center;
    font-size: 0.95rem;
  }
  .action.danger {
    color: var(--danger);
    margin-top: 0.4rem;
  }
</style>
