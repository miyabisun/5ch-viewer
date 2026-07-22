<script>
  import { onMount } from 'svelte'
  import { api } from './api.js'
  import ThreadRow from './ThreadRow.svelte'
  import ThreadMenu from './ThreadMenu.svelte'
  import Icon from './Icon.svelte'

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

  // Manual next-thread rescue from the archive list. The successor is registered as an active
  // (archived=0) favorite, so it won't show in this archive list — just show inline feedback.
  async function findNext(f) {
    const res = await api.findNext(f.server, f.board, f.thread_id)
    return res.found ? 'お気に入りに追加しました' : '次スレは見つかりませんでした'
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
    <button class="board-header" aria-expanded={open.has(g.key)} onclick={() => toggleGroup(g.key)}>
      <span class="board-name">{g.board_name}</span>
      <span class="board-count">({g.threads.length})</span>
      <span class="chevron" aria-hidden="true">
        <Icon name={open.has(g.key) ? 'chevron-up' : 'chevron-down'} size="16" />
      </span>
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
  <ThreadMenu {menu} onclose={closeMenu} onremoved={load} onfindnext={findNext}>
    {#snippet actions(f)}
      <button class="action" onclick={() => unarchive(f)}>アーカイブ解除</button>
    {/snippet}
  </ThreadMenu>
{/if}

<style>
  .empty {
    color: var(--muted);
    font-size: 14px;
    text-align: center;
    margin-top: 24px;
  }
  .board-group {
    margin-bottom: 8px;
  }
  /* List-row style header: surface-raised card with an 8px radius. */
  .board-header {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    color: var(--on-surface);
    font-size: 15px;
    text-align: left;
  }
  .board-header:hover {
    background: var(--border);
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
    font-size: 12px;
    flex: none;
  }
  .chevron {
    color: var(--muted);
    display: flex;
    align-items: center;
    flex: none;
  }
  .board-threads {
    padding-left: 8px;
    padding-top: 4px;
  }
</style>
