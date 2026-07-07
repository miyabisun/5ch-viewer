<script>
  // Shared action menu for a thread (right-click PC / long-press mobile).
  // Renders the modal shell, the thread URL, the copy section, an optional
  // "アーカイブ" button (when `onarchive` is given) and the danger "削除" button.
  // Callers inject type-specific actions (e.g. rating) via the `actions`
  // snippet, which receives the active thread.
  import { api } from './api.js'
  import Modal from './Modal.svelte'

  let { menu, onclose, onremoved, actions, onarchive, onfindnext } = $props()

  // Inline feedback for the "次スレを検索" action (no toast infra in this app).
  // onfindnext(menu) is expected to resolve to a message string to show here.
  let finding = $state(false)
  let findStatus = $state('')

  async function runFindNext() {
    if (finding) return
    finding = true
    findStatus = ''
    try {
      findStatus = await onfindnext(menu)
    } catch (e) {
      findStatus = e.message || '次スレの検索に失敗しました'
    } finally {
      finding = false
    }
  }

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

    {#if onfindnext}
      <div class="section-label">次スレ</div>
      <button class="action" disabled={finding} onclick={runFindNext}>次スレを検索</button>
      {#if findStatus}
        <div class="find-status" data-testid="find-next-status">{findStatus}</div>
      {/if}
    {/if}

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
    gap: 8px;
    width: 100%;
  }
  .menu-url {
    font-size: 12px;
    color: var(--muted);
    word-break: break-all;
    margin-top: 4px;
  }
  .find-status {
    font-size: 14px;
    color: var(--muted);
    padding: 2px 4px;
  }
  /* .action / .section-label styling comes from the shared .menu recipes in App.svelte. */
</style>
