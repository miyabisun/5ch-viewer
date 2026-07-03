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
  /* List row: card style — 8px radius, surface-raised, 1px border (DESIGN.md). */
  .thread {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    /* The left edge color bar is supplied by the caller via --row-color. */
    border-left: 4px solid var(--row-color, var(--muted));
    border-radius: 8px;
    margin-bottom: 4px;
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
    font-size: 15px;
    font-weight: 600;
    line-height: 1.3;
  }
  .sub {
    font-size: 12px;
    color: var(--muted);
  }
  /* Unread badge: dark-red full-radius pill, white text. Hidden when 0 (not rendered). */
  .unread {
    flex: none;
    background: var(--badge-bg);
    color: var(--badge-fg);
    font-size: 12px;
    font-weight: bold;
    line-height: 1;
    padding: 4px 8px;
    border-radius: 9999px;
  }
</style>
