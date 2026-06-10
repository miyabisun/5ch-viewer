<script>
  import { onMount } from 'svelte'
  import { api } from './lib/api.js'
  import { initTheme } from './lib/theme.js'
  import { parseLocation, push, replace } from './lib/router.js'
  import NavBar from './lib/NavBar.svelte'
  import FavoritesList from './lib/FavoritesList.svelte'
  import RegisterThread from './lib/RegisterThread.svelte'
  import ThreadView from './lib/ThreadView.svelte'

  let favorites = $state([])
  let current = $state(null)
  let error = $state(null)
  let page = $state('favorites') // 'favorites' | 'register'

  async function load() {
    try {
      favorites = await api.listFavorites()
      error = null
    } catch (e) {
      error = e.message
    }
  }

  // Find the matching favorite for a thread descriptor, or build a minimal
  // fav so a thread URL still opens even when it is not in the list.
  function favFor({ server, board, thread_id }) {
    const found = favorites.find(
      (f) => f.server === server && f.board === board && f.thread_id === thread_id,
    )
    return found ?? { server, board, thread_id, title: '', read_res: 0 }
  }

  // Apply a route descriptor (from initial load or popstate) to the UI state.
  function applyRoute(route) {
    page = route.page
    current = route.thread ? favFor(route.thread) : null
  }

  onMount(() => {
    initTheme()
    // Load favorites first so a thread URL can resolve to a real fav, then
    // apply the initial route. Use replaceState to normalize the entry.
    const route = parseLocation()
    replace(route)
    load().then(() => applyRoute(route))

    const onpop = () => applyRoute(parseLocation())
    window.addEventListener('popstate', onpop)
    return () => window.removeEventListener('popstate', onpop)
  })

  function open(fav) {
    current = fav
    // On wide screens the favorites list stays in the left column, so opening a
    // thread is a "favorites + thread" route. On narrow screens it is the
    // full-screen thread view. The route descriptor is the same either way.
    push({ page: 'favorites', thread: fav })
  }
  function back() {
    current = null
    push({ page })
    load()
  }
  function navigate(p) {
    page = p
    current = null
    push({ page: p })
  }

  // A `keyed` id forces ThreadView to remount when the selected thread changes.
  // Without this, switching threads in the persistent right column (PC) would
  // reuse the component instance and skip the open-time auto-refresh / restore.
  const threadKey = $derived(
    current ? `${current.server}/${current.board}/${current.thread_id}` : null,
  )
</script>

<NavBar {page} onnavigate={navigate} />

<main class="layout" class:has-thread={!!current}>
  {#if error}
    <p class="error">{error}</p>
  {/if}

  <!--
    Two-pane layout. On narrow screens CSS shows exactly one pane (single view);
    on wide screens both panes are shown side by side (left list / right detail).
    Both panes are always mounted so state stays shared and there is no double
    rendering or duplicate event wiring — visibility is purely CSS-driven.
  -->
  <section class="pane list-pane">
    {#if page === 'register'}
      <RegisterThread onchange={load} />
    {:else}
      <FavoritesList {favorites} onopen={open} onchange={load} />
    {/if}
  </section>

  <section class="pane detail-pane">
    {#if current}
      {#key threadKey}
        <ThreadView fav={current} onback={back} />
      {/key}
    {:else}
      <p class="placeholder">スレッドを選択してください</p>
    {/if}
  </section>
</main>

<style>
  :global(:root) {
    --bg: #fafafa;
    --fg: #222;
    --muted: #888;
    --border: #eee;
    --card-bg: #fff;
    --nav-bg: #fff;
    --accent: #e0a000;
    --danger: #c00;
    --error-bg: #fee;
    --name: #060;
    --link: #1a6;
    /* Unread badge: dark-red pill with white text. */
    --badge-bg: #a01818;
    --badge-fg: #fff;
    /* Rating color bar (★5..★1, 0=none). Tuned for the light theme. */
    --rate-0: #bbb;
    --rate-1: #29b6d8;
    --rate-2: #3fae5a;
    --rate-3: #e0c000;
    --rate-4: #ef8c00;
    --rate-5: #e23b3b;
  }
  :global([data-theme='dark']) {
    --bg: #1a1a1a;
    --fg: #e6e6e6;
    --muted: #999;
    --border: #333;
    --card-bg: #242424;
    --nav-bg: #202020;
    --accent: #e0a000;
    --danger: #ff6b6b;
    --error-bg: #3a1a1a;
    --name: #5bbf7a;
    --link: #4dd0a0;
    /* Unread badge: dark-red pill with white text. */
    --badge-bg: #8c1f1f;
    --badge-fg: #fff;
    /* Slightly brighter on dark so the bar stays visible. */
    --rate-0: #555;
    --rate-1: #4dd6f0;
    --rate-2: #57c46f;
    --rate-3: #f0d020;
    --rate-4: #ff9e1f;
    --rate-5: #ff5a5a;
  }
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    background: var(--bg);
    color: var(--fg);
  }
  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 0.5rem;
  }
  .pane {
    min-width: 0;
  }

  /*
    Narrow (phone): single view. Exactly one pane shows — the list by default,
    the detail full-width once a thread is open. The "選択してください"
    placeholder is desktop-only.
  */
  .detail-pane,
  .layout.has-thread .list-pane,
  .placeholder {
    display: none;
  }
  .layout.has-thread .detail-pane {
    display: block;
  }

  /*
    Wide (PC, >=768px): classic 2ch-style two columns — favorites list pinned on
    the left, thread detail on the right. Both panes are always visible; the
    placeholder fills the right column until a thread is selected.
  */
  @media (min-width: 768px) {
    main {
      max-width: 1100px;
      display: grid;
      grid-template-columns: minmax(18rem, 22rem) 1fr;
      gap: 0.75rem;
      align-items: start;
    }
    .error {
      grid-column: 1 / -1;
    }
    .detail-pane,
    .layout.has-thread .list-pane,
    .placeholder {
      display: block;
    }
    .placeholder {
      color: var(--muted);
      text-align: center;
      margin-top: 3rem;
    }
  }
  .error {
    color: var(--danger);
    background: var(--error-bg);
    padding: 0.5rem;
    border-radius: 4px;
  }
</style>
