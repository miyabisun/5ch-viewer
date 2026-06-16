<script>
  import { onMount } from 'svelte'
  import { api } from './lib/api.js'
  import { initTheme } from './lib/theme.js'
  import { parseLocation, push, replace } from './lib/router.js'
  import NavBar from './lib/NavBar.svelte'
  import FavoritesList from './lib/FavoritesList.svelte'
  import RegisterThread from './lib/RegisterThread.svelte'
  import ThreadView from './lib/ThreadView.svelte'
  import ArchiveList from './lib/ArchiveList.svelte'

  let favorites = $state([])
  let current = $state(null)
  let error = $state(null)
  let page = $state('favorites') // 'favorites' | 'register' | 'archive'
  // Global NG ID set: loaded once on mount, updated on change via onngchange callback.
  let ngIds = $state(new Set())
  // Global NG wacchoi list: array of { suffix, board, week_key, wacchoi, created_at }.
  // Matching is done client-side with (suffix + board + week_key) triple.
  let ngWacchoi = $state([])

  async function load() {
    try {
      favorites = await api.listFavorites()
      error = null
    } catch (e) {
      error = e.message
    }
  }

  async function loadNgIds() {
    try {
      const list = await api.listNgIds()
      ngIds = new Set(list.map((x) => x.ng_id))
    } catch {
      /* non-critical; NG filtering just won't apply */
    }
  }

  async function loadNgWacchoi() {
    try {
      ngWacchoi = await api.listNgWacchoi()
    } catch {
      /* non-critical; NG wacchoi filtering just won't apply */
    }
  }

  // Board-level prefetch then re-list so freshly downloaded counts surface.
  //
  // The server returns from /refresh *immediately* and does the heavy work
  // (subject.txt per board + bulk dat DL) in the background, so re-listing the
  // instant /refresh resolves would race the DL and still show stale counts.
  // We therefore re-list after a short delay to let the background DL land in the
  // DB. This is best-effort surfacing, not a correctness guarantee: very slow DLs
  // may not be reflected until the next manual list refresh (e.g. reopening the
  // list), which is acceptable for a warm-on-open hint.
  //
  // Fire-and-forget: a refresh failure must not block the list (the stored
  // favorites still render). The timer is cleared on unmount.
  const REFRESH_RELIST_DELAY_MS = 1500
  let relistTimer = null
  function refreshAndReload() {
    api
      .refreshFavorites()
      .then(() => {
        relistTimer = setTimeout(() => load(), REFRESH_RELIST_DELAY_MS)
      })
      .catch((e) => console.error('[refresh]', e))
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
    load().then(() => {
      applyRoute(route)
      // Warm favorites in the background (board-level subject + bulk dat).
      refreshAndReload()
    })
    loadNgIds()
    loadNgWacchoi()

    const onpop = () => applyRoute(parseLocation())
    window.addEventListener('popstate', onpop)
    return () => {
      window.removeEventListener('popstate', onpop)
      if (relistTimer) clearTimeout(relistTimer)
    }
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

  // Immediately reflect the read progress into the favorites list so the unread
  // badge in ThreadRow updates without waiting for a full re-fetch.
  // Called by ThreadView via the onprogress prop every time maxRead advances.
  function onProgress(readRes) {
    if (!current) return
    const i = favorites.findIndex(
      (f) => f.server === current.server && f.board === current.board && f.thread_id === current.thread_id,
    )
    if (i < 0) return // thread not in favorites list (minimal fav fallback)
    const cur = favorites[i].read_res ?? 0
    const next = Math.max(cur, readRes)
    if (next === cur) return
    // Replace the element (not mutate) so Svelte's reactivity picks up the change
    // and ThreadRow's derived `unread = res_count - read_res` recomputes.
    favorites[i] = { ...favorites[i], read_res: next }
  }
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
    {:else if page === 'archive'}
      <ArchiveList onopen={open} />
    {:else}
      <FavoritesList {favorites} onopen={open} onchange={load} />
    {/if}
  </section>

  <section class="pane detail-pane">
    {#if current}
      {#key threadKey}
        <ThreadView fav={current} onback={back} onprogress={onProgress} {ngIds} onngchange={loadNgIds} {ngWacchoi} onngwacchoichange={loadNgWacchoi} />
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
    /* NavBar height token: shared by NavBar layout and main PC height calc. */
    --navbar-h: 3.2rem;
    /* ID badge colours (light theme). l1=none (ID hidden), l2-l5=blue→red. */
    --id-l2: #1a6fd8;
    --id-l3: #8a4fd8;
    --id-l4: #e84d9e;
    --id-l5: #e23b3b;
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
    /* ID badge colours (dark theme): brighter to stay readable on dark backgrounds. */
    --id-l2: #4d9ff0;
    --id-l3: #a878f0;
    --id-l4: #ff7ac0;
    --id-l5: #ff5a5a;
    /* Slightly brighter on dark so the bar stays visible. */
    --rate-0: #555;
    --rate-1: #4dd6f0;
    --rate-2: #57c46f;
    --rate-3: #f0d020;
    --rate-4: #ff9e1f;
    --rate-5: #ff5a5a;
  }
  :global(html),
  :global(body) {
    /* Wall for mobile (window scroll): prevent native overscroll bounce at the
       bottom so the pull-to-refresh gesture can take over cleanly. */
    overscroll-behavior-y: contain;
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

    Each pane gets its own scroll container so that scrolling in the detail pane
    (e.g. restore-to-read-position) does not move the list pane. The main grid
    is fixed to the viewport height (below NavBar, height = --navbar-h) with
    overflow:hidden; panes fill that height and scroll independently.
  */
  @media (min-width: 768px) {
    main {
      max-width: 1100px;
      /* Fix main to viewport height below NavBar. box-sizing:border-box ensures
         the base padding (0.5rem) does not overflow the calculated height.
         Fallback 100vh for browsers without dvh support (PC only, so mobile
         address-bar resize is not a concern). */
      box-sizing: border-box;
      padding: 0;
      height: calc(100vh - var(--navbar-h, 3.2rem));
      height: calc(100dvh - var(--navbar-h, 3.2rem));
      overflow: hidden;
      display: grid;
      grid-template-columns: minmax(18rem, 22rem) 1fr;
      gap: 0.75rem;
      /* stretch (default) so panes fill the grid row height */
      align-items: stretch;
    }
    .error {
      grid-column: 1 / -1;
    }
    /* Independent scroll per pane. min-height:0 prevents grid items from
       refusing to shrink below their content height (grid item default is auto). */
    .list-pane,
    .detail-pane {
      overflow-y: auto;
      height: 100%;
      min-height: 0;
    }
    /* Wall (stage 1 of pull-to-refresh): prevent native rubber-band/bounce on the
       detail pane so the bottom overscroll is fully managed by the gesture code.
       Applied only to detail-pane (not list-pane) to leave list scrolling unaffected. */
    .detail-pane {
      overscroll-behavior: contain;
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
