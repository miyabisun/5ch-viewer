<script>
  // Reusable modal: close via the top-right ×, a scrim (outside) click, or Esc.
  // The bottom "閉じる" button convention is intentionally dropped in favor of
  // this shared behavior so all modals (anchor tree, favorites menu, ...) match.
  let { onclose, children, header } = $props()

  function onKey(e) {
    if (e.key === 'Escape') onclose()
  }
  // Close only when the scrim itself (not the dialog) is clicked. Checking the
  // target avoids a click handler on the dialog (a11y) while still closing on
  // an outside click.
  function onScrimClick(e) {
    if (e.target === e.currentTarget) onclose()
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- Scrim: clicking outside the dialog closes it. -->
<div class="modal-bg" role="presentation" onclick={onScrimClick}>
  <!-- Dialog: clicks inside stay inside (no onclick handler -> no bubbling close). -->
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
    <div class="modal-head">
      <div class="modal-head-slot">{@render header?.()}</div>
      <button class="modal-close" aria-label="閉じる" onclick={onclose}>×</button>
    </div>
    <div class="modal-content">
      {@render children()}
    </div>
  </div>
</div>

<style>
  .modal-bg {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 50;
  }
  .modal {
    background: var(--card-bg);
    border-radius: 8px;
    padding: 1rem;
    max-width: 100%;
    max-height: 80%;
    overflow: auto;
    position: relative;
  }
  .modal-head {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }
  .modal-head-slot {
    flex: 1;
    min-width: 0;
  }
  .modal-close {
    flex: none;
    margin-left: auto;
    width: 2rem;
    height: 2rem;
    line-height: 1;
    font-size: 1.3rem;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--fg);
    cursor: pointer;
  }
  .modal-close:hover {
    background: var(--border);
  }
</style>
