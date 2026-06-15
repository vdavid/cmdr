<script lang="ts">
  /**
   * Inline PDF view for the file viewer's `pdf` kind. WKWebView's PDFKit-backed
   * `<embed>` supplies its own scroll / zoom / page UI, so we only fill the content
   * area and surface a brief loading spinner.
   */
  import Spinner from '$lib/ui/Spinner.svelte'

  type Props = {
    /** `cmdr-media://localhost/<token>` URL for the PDF bytes. */
    src: string
    /** File name, surfaced on the embed for assistive tech. */
    fileName: string
  }

  const { src, fileName }: Props = $props()

  // `<embed>` doesn't reliably fire `load` (and almost never `error`) in WebKit, so the
  // spinner can't depend on its events alone or it would hang forever over a PDF that
  // already rendered. PDFKit paints fast, so clear the spinner on a short bounded
  // fallback, and sooner if `load` does fire. A genuinely broken PDF (or a 404 from the
  // scheme) surfaces PDFKit's own in-embed message rather than a separate error state.
  let loaded = $state(false)

  $effect(() => {
    const timer = setTimeout(() => {
      loaded = true
    }, 600)
    return () => { clearTimeout(timer); }
  })

  function handleLoad(): void {
    loaded = true
  }
</script>

<div class="media-pdf-stage">
  {#if !loaded}
    <div class="media-status" role="status">
      <Spinner size="lg" />
      <span class="sr-only">Loading PDF</span>
    </div>
  {/if}
  <embed class="media-pdf" type="application/pdf" {src} title={fileName} aria-label={fileName} onload={handleLoad} />
</div>

<style>
  .media-pdf-stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    background-color: var(--color-bg-primary);
  }

  .media-pdf {
    flex: 1;
    width: 100%;
    height: 100%;
    border: none;
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
</style>
