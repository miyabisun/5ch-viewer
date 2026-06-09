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
</script>

<NavBar {page} onnavigate={navigate} />

<main>
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if current}
    <ThreadView fav={current} onback={back} />
  {:else if page === 'register'}
    <RegisterThread onchange={load} />
  {:else}
    <FavoritesList {favorites} onopen={open} onchange={load} />
  {/if}
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
  .error {
    color: var(--danger);
    background: var(--error-bg);
    padding: 0.5rem;
    border-radius: 4px;
  }
</style>
