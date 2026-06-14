<script>
  // Shared action menu for a thread (right-click PC / long-press mobile).
  // Renders the modal shell, the thread URL, the copy section, an optional
  // "アーカイブ" button (when `onarchive` is given) and the danger "削除" button.
  // Callers inject type-specific actions (e.g. rating) via the `actions`
  // snippet, which receives the active thread.
  import { api } from './api.js'
  import Modal from './Modal.svelte'

  let { menu, onclose, onremoved, actions, onarchive } = $props()

  // 5ch thread URL (docs/5ch-spec.md): https://{server}.5ch.io/test/read.cgi/{board}/{thread_id}/
  function threadUrl(f) {
    return `https://${f.server}.5ch.io/test/read.cgi/${f.board}/${f.thread_id}/`
  }

  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      /* clipboard may be unavailable; fail silently */
    }
    onclose()
  }

  async function remove(f) {
    if (!confirm(`削除しますか？\n${f.title}`)) return
    await api.removeFavorite(f.server, f.board, f.thread_id)
    onclose()
    onremoved()
  }
</script>

<Modal {onclose}>
  {#snippet header()}
    {menu.title || '(未取得)'}
  {/snippet}

  <!-- .menu is the action body and the stable E2E hook for the open menu. -->
  <div class="menu">
    <div class="menu-url">{threadUrl(menu)}</div>

    {@render actions(menu)}

    <div class="section-label">コピー</div>
    <button class="action" onclick={() => copy(menu.title)}>タイトルをコピー</button>
    <button class="action" onclick={() => copy(threadUrl(menu))}>URL をコピー</button>
    <button class="action" onclick={() => copy(`${menu.title}\n${threadUrl(menu)}`)}>
      タイトル+URL をコピー
    </button>

    <div class="section-label">整理</div>
    {#if onarchive}
      <button class="action" onclick={() => onarchive(menu)}>アーカイブ</button>
    {/if}
    <button class="action danger" onclick={() => remove(menu)}>削除</button>
  </div>
</Modal>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    width: 100%;
  }
  .menu-url {
    font-size: 0.75rem;
    color: var(--muted);
    word-break: break-all;
    margin-top: 0.15rem;
  }
  :global(.menu .section-label) {
    font-size: 0.75rem;
    color: var(--muted);
    margin-top: 0.4rem;
  }
  :global(.menu .action) {
    width: 100%;
    padding: 0.7rem;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--fg);
    cursor: pointer;
    text-align: center;
    font-size: 0.95rem;
  }
  :global(.menu .action.danger) {
    color: var(--danger);
    margin-top: 0.4rem;
  }
</style>
