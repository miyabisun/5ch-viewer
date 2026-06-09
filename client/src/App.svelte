<script>
  import { onMount } from 'svelte'
  import { api } from './lib/api.js'
  import { initTheme } from './lib/theme.js'
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

  onMount(() => {
    initTheme()
    load()
  })

  function open(fav) {
    current = fav
  }
  function back() {
    current = null
    load()
  }
  function navigate(p) {
    page = p
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
