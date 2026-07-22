<script>
  import { currentTheme, toggleTheme } from './theme.js'
  import Icon from './Icon.svelte'

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
    <button
      class="tab"
      class:active={page === 'archive'}
      data-testid="tab-archive"
      onclick={() => onnavigate('archive')}
    >
      アーカイブ
    </button>
  </div>
  <button class="btn icon-btn" data-testid="theme-toggle" aria-label="テーマ切替" onclick={flip}>
    <Icon name={theme === 'dark' ? 'sun' : 'moon'} size="18" />
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
    gap: 8px;
    /* Use box-sizing:border-box so padding is included in --navbar-h. */
    box-sizing: border-box;
    height: var(--navbar-h, 3.2rem);
    padding: 4px 8px;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  /* Tabs: label type, muted when inactive, on-surface + accent underline when active. */
  .tab {
    border: none;
    background: none;
    color: var(--muted);
    padding: 8px 12px;
    cursor: pointer;
    font-size: 15px;
    font-weight: 500;
    line-height: 1.2;
    border-bottom: 2px solid transparent;
  }
  .tab.active {
    color: var(--on-surface);
    border-bottom-color: var(--accent);
  }
</style>
