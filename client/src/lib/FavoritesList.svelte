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

  async function setRating(f, rating) {
    await api.setRating(f.server, f.board, f.thread_id, rating)
    onchange()
  }

  async function remove(f) {
    if (!confirm(`削除しますか？\n${f.title}`)) return
    await api.removeFavorite(f.server, f.board, f.thread_id)
    onchange()
  }
</script>

{#each groups as [rating, threads] (rating)}
  <h2>{rating > 0 ? '★'.repeat(rating) : '☆ 未分類'}</h2>
  {#each threads as f (f.server + f.board + f.thread_id)}
    {@const unread = f.res_count - f.read_res}
    <div class="thread" class:dead={f.status === 'dead'}>
      <div
        class="info"
        role="button"
        tabindex="0"
        onclick={() => onopen(f)}
        onkeydown={(e) => e.key === 'Enter' && onopen(f)}
      >
        <div class="title">{f.title || '(未取得 — 開いて更新)'}</div>
        <div class="sub">{f.board_name} {f.res_count}</div>
      </div>
      {#if unread > 0}
        <span class="unread">[{unread}]</span>
      {/if}
      <select value={f.rating} onchange={(e) => setRating(f, +e.target.value)}>
        {#each [0, 1, 2, 3, 4, 5] as r}
          <option value={r}>{r}</option>
        {/each}
      </select>
      <button class="del" onclick={() => remove(f)}>×</button>
    </div>
  {/each}
{/each}

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
    background: var(--card-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-bottom: 0.3rem;
  }
  .thread.dead {
    opacity: 0.5;
  }
  .info {
    flex: 1;
    cursor: pointer;
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
  .del {
    border: none;
    background: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 1.1rem;
  }
</style>
