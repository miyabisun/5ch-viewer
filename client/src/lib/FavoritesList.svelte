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
</script>

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

<style>
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
</style>
