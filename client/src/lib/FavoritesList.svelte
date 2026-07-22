<script>
  import { api } from './api.js'
  import ThreadRow from './ThreadRow.svelte'
  import ThreadMenu from './ThreadMenu.svelte'

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

  // Manual next-thread rescue. On success reload the list so the new active thread appears;
  // returns an inline status message shown inside the menu (menu stays open on not-found).
  async function findNext(f) {
    const res = await api.findNext(f.server, f.board, f.thread_id)
    if (res.found) {
      onchange()
      return '次スレを追加しました'
    }
    return '次スレは見つかりませんでした'
  }
</script>

<!-- Dat updates are the auto-crawl's job (src/sync.rs); the list is display-only,
     so there is no manual refresh UI (needless 5ch load). -->
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
  <ThreadMenu
    {menu}
    onclose={closeMenu}
    onremoved={onchange}
    onarchive={archive}
    onfindnext={findNext}
  >
    {#snippet actions(f)}
      <div class="section-label">お気に入りレベル</div>
      <!--
        Star rating: 6 buttons total. Leftmost (r=0) is ☆ and clears the rating.
        r=1..5 always show ★; selection is conveyed by COLOR — .on (yellow) when
        f.rating >= r, .off (gray) otherwise — not by glyph shape.
      -->
      <div class="stars">
        {#each [0, 1, 2, 3, 4, 5] as r}
          {@const lit = r > 0 && f.rating >= r}
          <a
            href="#rate-{r}"
            class="star"
            class:on={lit}
            class:off={!lit}
            data-rating={r}
            aria-label={r === 0 ? 'お気に入り解除' : `レベル ${r}`}
            onclick={(e) => {
              e.preventDefault()
              setRating(f, r)
            }}>{r === 0 ? '☆' : '★'}</a
          >
        {/each}
      </div>
    {/snippet}
  </ThreadMenu>
{/if}

<style>
  /* Group header: ★ repeated per rating — data viz, so it uses --star-on (gold),
     not the chrome accent. */
  h2 {
    font-size: 15px;
    font-weight: 600;
    color: var(--star-on);
    margin: 16px 0 4px;
  }

  /* Star rating: color, not underline, conveys selection. */
  .stars {
    display: flex;
    justify-content: center;
    gap: 4px;
    font-size: 24px;
    line-height: 1;
  }
  .star {
    text-decoration: none;
    cursor: pointer;
  }
  .star.on {
    color: var(--star-on);
  }
  .star.off {
    color: var(--rate-0);
  }
</style>
