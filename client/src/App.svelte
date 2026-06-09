<script>
  import { onMount } from 'svelte'
  import { api } from './lib/api.js'
  import ThreadList from './lib/ThreadList.svelte'
  import ThreadView from './lib/ThreadView.svelte'

  let favorites = $state([])
  let current = $state(null)
  let error = $state(null)

  async function load() {
    try {
      favorites = await api.listFavorites()
      error = null
    } catch (e) {
      error = e.message
    }
  }

  onMount(load)

  function open(fav) {
    current = fav
  }
  function back() {
    current = null
    load()
  }
</script>

<main>
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if current}
    <ThreadView fav={current} onback={back} />
  {:else}
    <ThreadList {favorites} onopen={open} onchange={load} />
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    background: #fafafa;
    color: #222;
  }
  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 0.5rem;
  }
  .error {
    color: #c00;
    background: #fee;
    padding: 0.5rem;
    border-radius: 4px;
  }
</style>
