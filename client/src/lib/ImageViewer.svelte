<script>
  import { imageSwipe } from './imageSwipe.js'

  // images: [{ href, path, url, resNum, indexInRes, globalIndex }]
  // initialIndex: which image to show first
  // mosaicUrls: Set<string> (URLs with mosaic=1)
  // onclose: () => void
  // onImageMenu: ({ url, mosaic }) => void — opens the image context menu
  let { images, initialIndex = 0, mosaicUrls = new Set(), onclose, onImageMenu } = $props()

  // We intentionally snapshot initialIndex into local state. The parent re-mounts
  // this component each time a viewer is opened (via {#if imageViewerState != null}),
  // so currentIndex always starts from the requested image. Updating initialIndex
  // without re-mounting is unsupported by design.
  // svelte-ignore state_referenced_locally
  let currentIndex = $state(initialIndex)

  const current = $derived(images[currentIndex])
  const isMosaic = $derived(mosaicUrls.has(current?.url))

  function prev() {
    if (currentIndex > 0) currentIndex--
  }
  function next() {
    if (currentIndex < images.length - 1) currentIndex++
  }

  // Keyboard navigation.
  function onKey(e) {
    if (e.key === 'Escape') onclose()
    else if (e.key === 'ArrowLeft') prev()
    else if (e.key === 'ArrowRight') next()
  }

  // Long-press detection for the image (touch devices, 500ms).
  let longPressTimer
  let longPressed = false

  function onImgPointerDown(e) {
    if (e.pointerType !== 'touch') return
    longPressed = false
    longPressTimer = setTimeout(() => {
      longPressed = true
      openMenu()
    }, 500)
  }
  function cancelLongPress() {
    clearTimeout(longPressTimer)
  }
  function onImgContextMenu(e) {
    e.preventDefault()
    openMenu()
  }

  function openMenu() {
    if (!current || !onImageMenu) return
    onImageMenu({ url: current.url, mosaic: mosaicUrls.has(current.url) })
  }

  // Swallow click after a long-press so the image viewer does not close.
  function onImgClick(e) {
    if (longPressed) {
      longPressed = false
      e.stopPropagation()
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<!-- Full-screen backdrop. Clicking the backdrop (not the image) closes the viewer. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="viewer-bg"
  role="presentation"
  onclick={(e) => { if (e.target === e.currentTarget) onclose() }}
  use:imageSwipe={() => ({ onPrev: prev, onNext: next, onClose: onclose })}
>
  <!-- × close button -->
  <button class="viewer-close" aria-label="閉じる" onclick={onclose}>×</button>

  {#if current}
    <!-- Image container: click on image should not close (handled by backdrop). -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="viewer-img-wrap" onclick={(e) => e.stopPropagation()}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <img
        src="/api/images/{current.path}"
        alt="画像 {current.globalIndex + 1}"
        class="viewer-img"
        class:mosaic={isMosaic}
        onpointerdown={onImgPointerDown}
        onpointerup={cancelLongPress}
        onpointerleave={cancelLongPress}
        onpointercancel={cancelLongPress}
        oncontextmenu={onImgContextMenu}
        onclick={onImgClick}
      />
    </div>
  {/if}

  <!-- Navigation buttons (hidden when only 1 image) -->
  {#if images.length > 1}
    <button
      class="viewer-nav viewer-prev"
      aria-label="前の画像"
      disabled={currentIndex === 0}
      onclick={(e) => { e.stopPropagation(); prev() }}
    >&#8249;</button>
    <button
      class="viewer-nav viewer-next"
      aria-label="次の画像"
      disabled={currentIndex === images.length - 1}
      onclick={(e) => { e.stopPropagation(); next() }}
    >&#8250;</button>
  {/if}

  <!-- Footer: position counter -->
  <div class="viewer-footer" role="presentation" onclick={(e) => e.stopPropagation()}>
    {currentIndex + 1} / {images.length}
  </div>
</div>

<style>
  .viewer-bg {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.92);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    user-select: none;
  }

  .viewer-close {
    position: absolute;
    top: 0.75rem;
    right: 0.75rem;
    width: 2.2rem;
    height: 2.2rem;
    font-size: 1.4rem;
    line-height: 1;
    border: none;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
    cursor: pointer;
    z-index: 101;
  }
  .viewer-close:hover {
    background: rgba(255, 255, 255, 0.24);
  }

  .viewer-img-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    max-width: 100vw;
    max-height: 100dvh;
  }

  .viewer-img {
    max-width: 100vw;
    max-height: 100dvh;
    object-fit: contain;
    border-radius: 2px;
  }

  /* Mosaic: strong blur applied in the viewer (no tap-to-reveal). */
  .viewer-img.mosaic {
    filter: blur(40px);
  }

  .viewer-nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 2.5rem;
    height: 5rem;
    font-size: 2rem;
    line-height: 1;
    border: none;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
    cursor: pointer;
    z-index: 101;
  }
  .viewer-nav:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.22);
  }
  .viewer-nav:disabled {
    opacity: 0.25;
    cursor: default;
  }

  .viewer-prev {
    left: 0.5rem;
  }
  .viewer-next {
    right: 0.5rem;
  }

  .viewer-footer {
    position: absolute;
    bottom: 0.75rem;
    left: 50%;
    transform: translateX(-50%);
    color: rgba(255, 255, 255, 0.7);
    font-size: 0.9rem;
    background: rgba(0, 0, 0, 0.4);
    padding: 0.2rem 0.6rem;
    border-radius: 4px;
    pointer-events: none;
  }
</style>
