<script>
  import { api } from './api.js'

  let { favorites, onopen, onchange } = $props()

  const collator = new Intl.Collator('ja', { numeric: true })

  let url = $state('')
  let mode = $state('url') // 'url' | 'search'
  let searchResults = $state([])
  let searching = $state(false)

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

  async function add() {
    const v = url.trim()
    if (!v) return
    if (mode === 'search') {
      await runSearch(v)
      return
    }
    try {
      await api.addFavorite({ url: v })
      url = ''
      onchange()
    } catch (e) {
      alert(e.message)
    }
  }

  async function runSearch(q) {
    searching = true
    searchResults = []
    try {
      searchResults = await api.search(q)
    } catch (e) {
      alert(e.message)
    } finally {
      searching = false
    }
  }

  async function addFromSearch(r) {
    try {
      await api.addFavorite({
        server: r.server,
        board: r.board,
        thread_id: r.thread_id,
        title: r.title,
      })
      // Remove the newly registered entry from the results.
      searchResults = searchResults.filter((x) => x !== r)
      onchange()
    } catch (e) {
      alert(e.message)
    }
  }

  // Clear search results and input when switching modes.
  function onModeChange(e) {
    mode = e.target.value
    searchResults = []
    url = ''
  }

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

<div class="add">
  <select value={mode} onchange={onModeChange}>
    <option value="search">スレタイ検索</option>
    <option value="url">URL指定</option>
  </select>
  <input
    placeholder={mode === 'url' ? 'スレッドURLを貼り付け' : 'キーワードで検索'}
    bind:value={url}
    onkeydown={(e) => e.key === 'Enter' && add()}
  />
  <button onclick={add} disabled={searching}>
    {mode === 'search' ? (searching ? '検索中…' : '検索') : '追加'}
  </button>
</div>

{#if mode === 'search' && searchResults.length > 0}
  <div class="results">
    {#each searchResults as r (r.server + r.board + r.thread_id)}
      <div class="result">
        <div class="result-info">
          <div class="title">{r.title}</div>
          <div class="sub">{r.board} {r.res_count}</div>
        </div>
        <button class="add-btn" onclick={() => addFromSearch(r)}>追加</button>
      </div>
    {/each}
  </div>
{/if}

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
  .add {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }
  .add input {
    flex: 1;
    padding: 0.4rem;
  }
  .results {
    margin-bottom: 1rem;
    border: 1px solid #ddd;
    border-radius: 6px;
    overflow: hidden;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    border-bottom: 1px solid #eee;
  }
  .result:last-child {
    border-bottom: none;
  }
  .result-info {
    flex: 1;
    min-width: 0;
  }
  .add-btn {
    padding: 0.3rem 0.6rem;
    cursor: pointer;
  }
  h2 {
    font-size: 1rem;
    color: #e0a000;
    margin: 1rem 0 0.3rem;
  }
  .thread {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    background: #fff;
    border: 1px solid #eee;
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
    color: #888;
  }
  .unread {
    color: #c00;
    font-weight: bold;
  }
  .del {
    border: none;
    background: none;
    color: #999;
    cursor: pointer;
    font-size: 1.1rem;
  }
</style>
