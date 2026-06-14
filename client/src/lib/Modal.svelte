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
    <!-- × button: absolutely positioned in the top-right corner so it never
         shifts content and stays in place while the inner content scrolls. -->
    <button class="modal-close" aria-label="閉じる" onclick={onclose}>×</button>
    <!-- Scrollable content area. padding-top reserves space so text never
         slides under the × button. header slot is rendered inside the scroll
         region so long titles are part of the scrollable flow. -->
    <div class="modal-content">
      {#if header}
        <div class="modal-header">{@render header()}</div>
      {/if}
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
    max-height: 80dvh;
    overflow: hidden;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  /* × button: pinned to top-right of .modal (position:relative).
     Kept visually subtle (muted color, transparent bg) so it does not
     compete with content. Hover gives a slight affordance. */
  .modal-close {
    position: absolute;
    top: 0.4rem;
    right: 0.4rem;
    z-index: 1;
    width: 1.8rem;
    height: 1.8rem;
    line-height: 1;
    font-size: 1.2rem;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
  }
  .modal-close:hover {
    color: var(--fg);
    background: var(--border);
  }
  /* Scrollable inner area. padding-top prevents content from sliding under
     the absolutely-positioned × button (approx button height + gap). */
  .modal-content {
    overflow-y: auto;
    padding-top: 1.75rem;
    flex: 1;
    min-height: 0;
  }
  .modal-header {
    margin-bottom: 0.5rem;
    /* Leave room on the right so long header text doesn't overlap the × button. */
    padding-right: 1.5rem;
  }
</style>
