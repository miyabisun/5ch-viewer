<script>
  import { onMount } from 'svelte'
  import { api } from './api.js'
  import ThreadRow from './ThreadRow.svelte'
  import ThreadMenu from './ThreadMenu.svelte'

  let { onopen } = $props()

  let archives = $state([])
  let error = $state(null)

  const collator = new Intl.Collator('ja', { numeric: true })

  async function load() {
    try {
      archives = await api.listArchives()
      error = null
    } catch (e) {
      error = e.message
    }
  }

  onMount(load)

  // Group by server+board key (avoids same-name board collision across servers).
  // Each group: { key, board_name, board, server, threads[] }, sorted by board_name.
  let groups = $derived.by(() => {
    const map = new Map()
    for (const f of archives) {
      const key = `${f.server}/${f.board}`
      if (!map.has(key)) {
        map.set(key, {
          key,
          board_name: f.board_name || f.board,
          board: f.board,
          server: f.server,
          threads: [],
        })
      }
      map.get(key).threads.push(f)
    }
    const entries = [...map.values()]
    // Sort groups by board_name (natural Japanese order).
    entries.sort((a, b) => collator.compare(a.board_name, b.board_name))
    // Sort threads within each group by title.
    for (const g of entries) {
      g.threads.sort((a, b) => collator.compare(a.title, b.title))
    }
    return entries
  })

  // Accordion open/close state per group key. Default: all collapsed.
  let open = $state(new Set())

  function toggleGroup(key) {
    const next = new Set(open)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    open = next
  }

  // --- Action menu (right-click PC / long-press mobile) ---
  let menu = $state(null)

  function openMenu(f) {
    menu = f
  }
  function closeMenu() {
    menu = null
  }

  async function unarchive(f) {
    await api.setArchived(f.server, f.board, f.thread_id, false)
    closeMenu()
    await load()
  }
</script>

{#if error}
  <p class="error">{error}</p>
{/if}

{#if archives.length === 0 && !error}
  <p class="empty">アーカイブはありません</p>
{/if}

{#each groups as g (g.key)}
  <div class="board-group">
    <button
      class="board-header"
      aria-expanded={open.has(g.key)}
      onclick={() => toggleGroup(g.key)}
    >
      <span class="board-name">{g.board_name}</span>
      <span class="board-count">({g.threads.length})</span>
      <span class="chevron" aria-hidden="true">{open.has(g.key) ? '▲' : '▼'}</span>
    </button>

    {#if open.has(g.key)}
      <div class="board-threads">
        {#each g.threads as f (f.server + f.board + f.thread_id)}
          <ThreadRow thread={f} {onopen} onmenu={openMenu} />
        {/each}
      </div>
    {/if}
  </div>
{/each}

{#if menu}
  <ThreadMenu {menu} onclose={closeMenu} onremoved={load}>
    {#snippet actions(f)}
      <button class="action" onclick={() => unarchive(f)}>アーカイブ解除</button>
    {/snippet}
  </ThreadMenu>
{/if}

<style>
  .error {
    color: var(--danger);
    background: var(--error-bg);
    padding: 0.5rem;
    border-radius: 4px;
  }
  .empty {
    color: var(--muted);
    text-align: center;
    margin-top: 3rem;
  }
  .board-group {
    margin-bottom: 0.5rem;
  }
  .board-header {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    padding: 0.5rem 0.6rem;
    background: var(--card-bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
    color: var(--fg);
    font-size: 0.95rem;
    text-align: left;
  }
  .board-header:hover {
    background: var(--bg);
  }
  .board-name {
    font-weight: 600;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .board-count {
    color: var(--muted);
    font-size: 0.85rem;
    flex: none;
  }
  .chevron {
    color: var(--muted);
    font-size: 0.7rem;
    flex: none;
  }
  .board-threads {
    padding-left: 0.5rem;
    padding-top: 0.25rem;
  }
</style>
