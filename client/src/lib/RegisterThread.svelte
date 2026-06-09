<script>
  import { api } from './api.js'

  let { onchange } = $props()

  let url = $state('')
  let mode = $state('search') // 'url' | 'search'
  let searchResults = $state([])
  let searching = $state(false)

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
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    border-bottom: 1px solid var(--border);
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
  .title {
    font-weight: 600;
  }
  .sub {
    font-size: 0.8rem;
    color: var(--muted);
  }
</style>
