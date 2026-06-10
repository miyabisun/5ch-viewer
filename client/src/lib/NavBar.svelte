<script>
  import { currentTheme, toggleTheme } from './theme.js'

  let { page, onnavigate } = $props()

  let theme = $state(currentTheme())

  function flip() {
    theme = toggleTheme()
  }
</script>

<nav>
  <div class="tabs">
    <button
      class="tab"
      class:active={page === 'favorites'}
      data-testid="tab-favorites"
      onclick={() => onnavigate('favorites')}
    >
      お気に入り
    </button>
    <button
      class="tab"
      class:active={page === 'register'}
      data-testid="tab-register"
      onclick={() => onnavigate('register')}
    >
      スレッド登録
    </button>
  </div>
  <button
    class="theme-toggle"
    data-testid="theme-toggle"
    aria-label="テーマ切替"
    onclick={flip}
  >
    {theme === 'dark' ? '☀' : '☾'}
  </button>
</nav>

<style>
  nav {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    /* Use box-sizing:border-box so padding is included in --navbar-h. */
    box-sizing: border-box;
    height: var(--navbar-h, 3.2rem);
    padding: 0.4rem 0.5rem;
    background: var(--nav-bg);
    border-bottom: 1px solid var(--border);
  }
  .tabs {
    display: flex;
    gap: 0.25rem;
  }
  .tab {
    border: none;
    background: none;
    color: var(--muted);
    padding: 0.4rem 0.6rem;
    cursor: pointer;
    font-size: 0.95rem;
    border-bottom: 2px solid transparent;
  }
  .tab.active {
    color: var(--fg);
    font-weight: 600;
    border-bottom-color: var(--accent);
  }
  .theme-toggle {
    border: 1px solid var(--border);
    background: var(--card-bg);
    color: var(--fg);
    border-radius: 6px;
    width: 2rem;
    height: 2rem;
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
  }
</style>
