<script>
  // Reusable modal: close via the top-right ×, a scrim (outside) click, or Esc.
  // The bottom "閉じる" button convention is intentionally dropped in favor of
  // this shared behavior so all modals (anchor tree, favorites menu, ...) match.
  import Icon from './Icon.svelte'

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
  <!-- Dialog: grid layout — row 1 has optional header (left) + × button (right, 30px fixed);
       row 2 spans full width and holds the scrollable content. -->
  <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
    <!-- Header cell: row 1, column 1. Absent when no header snippet is passed;
         the × button alone keeps the row height via minmax(30px, auto). -->
    {#if header}
      <div class="modal-header">{@render header()}</div>
    {/if}
    <!-- × button: row 1, column 2 (or column 1/-1 when no header). Quiet icon
         button (muted color, transparent bg) so it does not compete with content. -->
    <button class="modal-close" aria-label="閉じる" onclick={onclose}>
      <Icon name="x" size="20" />
    </button>
    <!-- Scrollable content area spanning both columns in row 2.
         Scrollbar is hidden (Firefox: scrollbar-width; WebKit: ::-webkit-scrollbar). -->
    <div class="modal-content">
      {@render children()}
    </div>
  </div>
</div>

<style>
  .modal-bg {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    z-index: 50;
  }
  /* Floating layer: 12px radius + the only sanctioned drop shadow (DESIGN.md). */
  .modal {
    background: var(--surface-raised);
    border-radius: 12px;
    padding: 16px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.25);
    max-width: 100%;
    overflow: hidden;
    /* Grid: column 1 = flexible title, column 2 = 36px × button.
       Row 1 = header row (min 36px so the × always has height), row 2 = content. */
    display: grid;
    grid-template-columns: 1fr 36px;
    grid-template-rows: minmax(36px, auto) 1fr;
    gap: 0 4px;
  }
  .modal-header {
    grid-row: 1;
    grid-column: 1;
    align-self: center;
    font-size: 17px;
    font-weight: 600;
    line-height: 1.3;
    word-break: break-word;
    padding-right: 4px;
  }
  /* × button: quiet icon button (36×36 hit area), grid cell (row 1, column 2).
     Kept visually subtle so it does not compete with content. Hover gives a slight affordance. */
  .modal-close {
    grid-row: 1;
    grid-column: 2;
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    align-self: center;
    justify-self: center;
  }
  .modal-close:hover {
    color: var(--on-surface);
    background: var(--border);
  }
  /* Scrollable content: spans both columns in row 2. Scrollbar hidden for clean look. */
  .modal-content {
    grid-row: 2;
    grid-column: 1 / -1;
    overflow-y: auto;
    max-height: 80dvh;
    scrollbar-width: none;
  }
  .modal-content::-webkit-scrollbar {
    display: none;
  }
  /* On touch devices the modal often appears under a still-pressed finger
     (long-press menus): without this the native selection latches onto the
     button labels. PC keeps selection for copyable modal content. */
  @media (hover: none) and (pointer: coarse) {
    .modal {
      user-select: none;
      -webkit-user-select: none;
      -webkit-touch-callout: none;
    }
  }
</style>
