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
    // Visiting the list makes NO 5ch access: render straight from SQLite (GET /api/favorites).
    // There is no manual refresh UI; subject.txt/dat updates are the background auto-crawl's
    // job (src/sync.rs). A browser pull-to-refresh just re-renders this way.
    load().then(() => applyRoute(route))
    loadNgIds()
    loadNgWacchoi()

    const onpop = () => applyRoute(parseLocation())
    window.addEventListener('popstate', onpop)
    return () => {
      window.removeEventListener('popstate', onpop)
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
  /* Sumi design tokens (DESIGN.md). Chrome colors come from here exclusively;
     components must never hardcode hex values. */
  :global(:root) {
    /* Native widgets (select dropdown, scrollbars) follow the active theme. */
    color-scheme: light;
    --surface: #fafafa;
    --surface-raised: #ffffff;
    --on-surface: #222222;
    /* Chrome text pushed past AAA (7:1) for e-paper grayscale legibility. */
    --muted: #4a4a4a;
    /* Darker hairline so row dividers survive e-paper gray quantization. */
    --border: #c9c9c4;
    --scrim: rgba(0, 0, 0, 0.4);
    --accent: #7a5400;
    --accent-subtle: rgba(122, 84, 0, 0.12);
    --link: #14506e;
    --danger: #8f1d16;
    --danger-subtle: #fdeeee;
    /* --- Functional data colors (project-domain, exempt from the one-accent rule) --- */
    --name: #005500;
    /* Star-rating glyph when lit (data viz, decoupled from the chrome accent). */
    --star-on: #8a6000;
    /* Unread badge: dark-red pill with white text. */
    --badge-bg: #a01818;
    --badge-fg: #fff;
    /* Rating color bar (★5..★1, 0=none). Washi darkness ramp for e-paper:
       lightness carries the level (monotonically darker), hue is secondary. */
    --rate-0: #9a9a9a;
    --rate-1: #1a8a9e;
    --rate-2: #1f7a33;
    --rate-3: #6e5a00;
    --rate-4: #7a3c00;
    --rate-5: #7a1414;
    /* NavBar height token: shared by NavBar layout and main PC height calc. */
    --navbar-h: 3.2rem;
    /* Unread new-post indicator (burnt orange). Separate from --danger (red) because unread is not an error. */
    --unread: #7a3c00;
    /* Own-post indicator (magenta). Takes priority over unread when both would apply. */
    --own: #8f1c5a;
    /* ID badge colours (Washi). l1=none (ID hidden), l2-l5 = darkness ramp
       (blue→purple→magenta→deep red), each ≥4.5:1 on white as text. */
    --id-l2: #2a6fc0;
    --id-l3: #7a3db4;
    --id-l4: #8f1c5a;
    --id-l5: #7a1414;
  }
  :global([data-theme='dark']) {
    color-scheme: dark;
    --surface: #191919;
    --surface-raised: #232323;
    --on-surface: #e6e6e6;
    --muted: #9a9a9a;
    --border: #333333;
    --scrim: rgba(0, 0, 0, 0.6);
    --accent: #e0a800;
    --accent-subtle: rgba(224, 168, 0, 0.15);
    --link: #7fdbff;
    --danger: #ff6b6b;
    --danger-subtle: #3a1a1a;
    /* --- Functional data colors (dark theme) --- */
    --name: #5bbf7a;
    --star-on: #e0a800;
    /* Unread / own indicators (dark theme). */
    --unread: #ff9e1f;
    --own: #ff7ac0;
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

  /* Shared focus ring (DESIGN.md): accent at 60% opacity on :focus-visible only.
     The UA default blue ring must never appear. */
  :global(:focus-visible) {
    outline: 2px solid color-mix(in srgb, var(--accent) 60%, transparent);
    outline-offset: 2px;
  }
  /* Inputs/textareas/selects: suppress the UA outline on focus in favor of an
     accent border (never remove focus indication without a substitute).
     :root raises specificity above per-component base border rules. */
  :global(:root input:focus),
  :global(:root textarea:focus),
  :global(:root select:focus) {
    outline: none;
    border-color: var(--accent);
  }

  /* --- Shared control recipes (DESIGN.md Components) ---
     Svelte styles are component-scoped, so the recipes reused across
     components live here (App is always mounted). Components keep only
     their layout-specific overrides. */
  /* Default button: surface-raised bg, 1px border, label type, 6px radius. */
  :global(.btn) {
    padding: 8px 14px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-raised);
    color: var(--on-surface);
    font-size: 15px;
    font-weight: 500;
    font-family: inherit;
    line-height: 1.2;
    cursor: pointer;
  }
  :global(.btn:hover:not(:disabled)) {
    background: var(--border);
  }
  :global(.btn:disabled) {
    opacity: 0.5;
    cursor: default;
  }
  /* Icon button (used with .btn): 36×36 hit area, SVG centered. */
  :global(.icon-btn) {
    width: 36px;
    height: 36px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  /* Input/textarea: surface bg (one level below the card/modal), body type. */
  :global(.input) {
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--on-surface);
    font-size: 16px;
    font-family: inherit;
  }
  :global(.input::placeholder) {
    color: var(--muted);
  }
  /* Menu actions: modal-presented stack of full-width default buttons
     (DESIGN.md Menus), with caption-muted section labels. */
  :global(.menu .action) {
    width: 100%;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-raised);
    color: var(--on-surface);
    cursor: pointer;
    text-align: center;
    font-size: 15px;
    font-weight: 500;
  }
  :global(.menu .action:hover) {
    background: var(--border);
  }
  /* Disabled action (e.g. find-next in flight): dim, no hover, default cursor. */
  :global(.menu .action:disabled) {
    opacity: 0.5;
    cursor: default;
  }
  :global(.menu .action:disabled:hover) {
    background: var(--surface-raised);
  }
  :global(.menu .action.danger) {
    color: var(--danger);
    margin-top: 8px;
  }
  :global(.menu .section-label) {
    font-size: 12px;
    color: var(--muted);
    margin-top: 8px;
  }
  /* Error banner: danger text on a danger-subtle tint. */
  :global(.error) {
    color: var(--danger);
    background: var(--danger-subtle);
    padding: 8px 12px;
    border-radius: 8px;
  }
  :global(body) {
    margin: 0;
    font-family: system-ui, sans-serif;
    font-size: 16px;
    line-height: 1.6;
    background: var(--surface);
    color: var(--on-surface);
  }
  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 8px;
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
    /* Phone: thread-view fills the viewport below NavBar so .thread-body can scroll. */
    height: calc(100vh - var(--navbar-h, 3.2rem));
    height: calc(100dvh - var(--navbar-h, 3.2rem));
    overflow: hidden;
  }
  /* Phone: when a thread is open, remove main's padding so detail-pane bottom
     does not overflow the viewport (padding-bottom would push it ~8px outside). */
  .layout.has-thread {
    padding: 0;
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
      gap: 12px;
      /* stretch (default) so panes fill the grid row height */
      align-items: stretch;
    }
    .error {
      grid-column: 1 / -1;
    }
    /* Independent scroll per pane. min-height:0 prevents grid items from
       refusing to shrink below their content height (grid item default is auto). */
    .list-pane {
      overflow-y: auto;
      height: 100%;
      min-height: 0;
    }
    /* detail-pane: overflow:hidden so .thread-view (height:100%) can take over
       scrolling internally via .thread-body.  The thread-view fills this pane. */
    .detail-pane {
      overflow: hidden;
      height: 100%;
      min-height: 0;
    }
    .detail-pane,
    .layout.has-thread .list-pane,
    .placeholder {
      display: block;
    }
    .placeholder {
      color: var(--muted);
      font-size: 14px;
      text-align: center;
      margin-top: 24px;
    }
  }
</style>
