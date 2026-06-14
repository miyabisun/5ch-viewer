<script>
  // A single thread row: title + sub line + unread badge.
  // Handles click-to-open, right-click (PC) and long-press (touch) to open a menu.
  let { thread: f, onopen, onmenu, extraClass = '', style = '', dataRating } = $props()

  const unread = $derived(f.res_count - f.read_res)

  // Long-press detection for touch devices.
  let pressTimer
  let longPressed = false
  function onPointerDown(e) {
    if (e.pointerType !== 'touch') return
    longPressed = false
    pressTimer = setTimeout(() => {
      longPressed = true
      onmenu(f)
    }, 500)
  }
  function cancelPress() {
    clearTimeout(pressTimer)
  }

  function onItemClick() {
    // Swallow the click that ends a long-press so the thread does not open.
    if (longPressed) {
      longPressed = false
      return
    }
    onopen(f)
  }
</script>

<div
  class="thread {extraClass}"
  class:dead={f.status === 'dead'}
  {style}
  data-rating={dataRating}
  role="button"
  tabindex="0"
  oncontextmenu={(e) => {
    e.preventDefault()
    onmenu(f)
  }}
  onpointerdown={onPointerDown}
  onpointerup={cancelPress}
  onpointerleave={cancelPress}
  onpointercancel={cancelPress}
  onclick={onItemClick}
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

<style>
  .thread {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    padding-left: 0.6rem;
    background: var(--card-bg);
    border: 1px solid var(--border);
    /* The left edge color bar is supplied by the caller via --row-color. */
    border-left: 4px solid var(--row-color, var(--muted));
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
</style>
