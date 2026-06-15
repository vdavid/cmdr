<script lang="ts">
  /**
   * Inline image view for the file viewer's `image` kind. Renders an `<img>` served
   * from the backend's `cmdr-media://` scheme (no virtual-scroll line machinery).
   *
   * Interactions (all keyboard-reachable):
   *   - Fit-to-window by default; click (or Enter / Space) toggles 100% / fit.
   *   - Scroll / pinch to zoom (`+` / `-` keys too); drag to pan; `0` resets to fit.
   *   - Checkerboard behind transparency, fixed in screen space.
   *
   * Loading + error states ship from the start: a spinner until `load` / `error`
   * fires, and a friendly inline message on `error`.
   */
  import Spinner from '$lib/ui/Spinner.svelte'
  import { clampZoom, nextClickZoom, type ImageViewMode } from './media-view'

  type Props = {
    /** `cmdr-media://localhost/<token>` URL for the image bytes. */
    src: string
    /** File name, used as the image's alt text. */
    fileName: string
  }

  const { src, fileName }: Props = $props()

  type LoadState = 'loading' | 'loaded' | 'error'
  let loadState = $state<LoadState>('loading')

  // `fit` lets the `<img>` shrink to the viewport via CSS; `actual` honors `zoom`.
  let mode = $state<ImageViewMode>('fit')
  let zoom = $state(1)
  let panX = $state(0)
  let panY = $state(0)

  // Drag-to-pan state. Only active in `actual` mode (fit has nothing to pan).
  let dragging = $state(false)
  let dragPointerId: number | null = null
  let dragStartX = 0
  let dragStartY = 0
  let dragStartPanX = 0
  let dragStartPanY = 0

  function resetToFit(): void {
    mode = 'fit'
    zoom = 1
    panX = 0
    panY = 0
  }

  function handleImgLoad(): void {
    loadState = 'loaded'
  }

  function handleImgError(): void {
    loadState = 'error'
  }

  function toggleClickZoom(): void {
    const next = nextClickZoom(mode)
    mode = next.mode
    if (next.zoom !== null) zoom = next.zoom
    if (next.mode === 'fit') {
      panX = 0
      panY = 0
    }
  }

  function zoomBy(factor: number): void {
    // Any zoom interaction switches out of fit so the explicit factor takes effect.
    if (mode === 'fit') mode = 'actual'
    zoom = clampZoom(zoom * factor)
  }

  function handleWheel(e: WheelEvent): void {
    // Ctrl/Cmd-less wheel still zooms here: the viewer image has nothing to scroll,
    // so a plain wheel reads as "zoom" (Preview.app behaves the same). preventDefault
    // stops the page from rubber-banding.
    e.preventDefault()
    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1
    zoomBy(factor)
  }

  function handleKeyDown(e: KeyboardEvent): void {
    if (e.metaKey || e.ctrlKey || e.altKey) return
    switch (e.key) {
      case 'Enter':
      case ' ':
        e.preventDefault()
        toggleClickZoom()
        break
      case '+':
      case '=':
        e.preventDefault()
        zoomBy(1.25)
        break
      case '-':
        e.preventDefault()
        zoomBy(1 / 1.25)
        break
      case '0':
        e.preventDefault()
        resetToFit()
        break
      case '1':
        e.preventDefault()
        mode = 'actual'
        zoom = 1
        break
    }
  }

  function handlePointerDown(e: PointerEvent): void {
    // Pan only in actual mode; in fit mode a pointerdown is a no-op (click toggles).
    if (mode !== 'actual') return
    if (e.button !== 0) return
    dragging = true
    dragPointerId = e.pointerId
    dragStartX = e.clientX
    dragStartY = e.clientY
    dragStartPanX = panX
    dragStartPanY = panY
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  }

  function handlePointerMove(e: PointerEvent): void {
    if (!dragging || e.pointerId !== dragPointerId) return
    panX = dragStartPanX + (e.clientX - dragStartX)
    panY = dragStartPanY + (e.clientY - dragStartY)
  }

  function endDrag(e: PointerEvent): void {
    if (e.pointerId !== dragPointerId) return
    dragging = false
    dragPointerId = null
  }

  // A small move means "click" (toggle zoom); a real drag pans and swallows the click.
  let pointerDownX = 0
  let pointerDownY = 0
  function recordPointerDown(e: PointerEvent): void {
    pointerDownX = e.clientX
    pointerDownY = e.clientY
    handlePointerDown(e)
  }
  function handleClick(e: MouseEvent): void {
    const moved = Math.abs(e.clientX - pointerDownX) + Math.abs(e.clientY - pointerDownY)
    if (moved > 4) return // was a drag, not a click
    toggleClickZoom()
  }

  const imgTransform = $derived(
    mode === 'fit'
      ? 'none'
      : `translate(${String(panX)}px, ${String(panY)}px) scale(${String(zoom)})`,
  )
</script>

<!--
  The stage is a focusable region so keyboard users can zoom/pan. role="img" with the
  file name as the label keeps the announcement meaningful while the interactive
  handlers live on the container.
-->
<div
  class="media-image-stage"
  class:actual={mode === 'actual'}
  class:dragging
  role="img"
  aria-label={fileName}
  tabindex="0"
  onwheel={handleWheel}
  onkeydown={handleKeyDown}
  onpointerdown={recordPointerDown}
  onpointermove={handlePointerMove}
  onpointerup={endDrag}
  onpointercancel={endDrag}
  onclick={handleClick}
>
  {#if loadState === 'loading'}
    <div class="media-status" role="status">
      <Spinner size="lg" />
      <span class="sr-only">Loading image</span>
    </div>
  {/if}
  {#if loadState === 'error'}
    <div class="media-status media-error" role="alert">
      <p>Sorry, we couldn't show this image. The file may be damaged or in a format we can't display.</p>
    </div>
  {/if}
  <!--
    Always in the DOM (even while loading) so the load / error events fire. Hidden
    until loaded so the broken-image glyph never flashes. The checkerboard sits behind
    it via the stage background, so transparency reads correctly.
  -->
  <img
    class="media-image"
    class:hidden={loadState !== 'loaded'}
    {src}
    alt={fileName}
    draggable="false"
    style="transform: {imgTransform}"
    onload={handleImgLoad}
    onerror={handleImgError}
  />
</div>

<style>
  .media-image-stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    outline: none;
    /* Checkerboard behind transparency, fixed in screen space (background-attachment
       isn't needed since the stage itself doesn't scroll). Two layered gradients make
       the classic 2-tone checker using design tokens. */
    background-color: var(--color-bg-primary);
    background-image:
      linear-gradient(45deg, var(--color-bg-tertiary) 25%, transparent 25%),
      linear-gradient(-45deg, var(--color-bg-tertiary) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, var(--color-bg-tertiary) 75%),
      linear-gradient(-45deg, transparent 75%, var(--color-bg-tertiary) 75%);
    background-size: 20px 20px;
    background-position:
      0 0,
      0 10px,
      10px -10px,
      -10px 0;
  }

  .media-image-stage:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: -2px;
  }

  .media-image-stage.actual {
    cursor: grab;
  }

  .media-image-stage.dragging {
    cursor: grabbing;
  }

  .media-image {
    max-width: 100%;
    max-height: 100%;
    /* WebKit applies EXIF orientation by default (CSS `image-orientation: from-image`),
       so phone photos display upright. Keep the default. */
    transform-origin: center center;
    user-select: none;
    -webkit-user-select: none;
  }

  @media (prefers-reduced-motion: no-preference) {
    .media-image {
      transition: transform var(--transition-base);
    }
  }

  /* While dragging, drop the transition so panning tracks the pointer 1:1. */
  .media-image-stage.dragging .media-image {
    transition: none;
  }

  .media-image.hidden {
    visibility: hidden;
  }

  .media-status {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-lg);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    text-align: center;
  }

  .media-error {
    color: var(--color-error);
  }

  .media-error p {
    margin: 0;
    max-width: 28rem;
    line-height: 1.4;
  }
</style>
