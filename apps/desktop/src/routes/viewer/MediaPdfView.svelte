<script lang="ts">
  /**
   * Inline PDF view for the file viewer's `pdf` kind. WKWebView's PDFKit-backed
   * `<embed>` supplies its own scroll / zoom / page UI, so we only fill the content
   * area and surface a brief loading spinner.
   *
   * Its text and selection live inside the embed, where neither the page nor the
   * viewer's own selection model reaches, so Edit > Copy and Select all (⌘C / ⌘A)
   * go to the native responder chain while this view is on screen.
   */
  import Spinner from '#lib/ui/Spinner.svelte'
  import { tString } from '#lib/intl/messages.svelte.js'
  import { onViewerEditAction, viewerForwardEditActionToPage } from '#lib/tauri-commands/index.js'
  import { getAppLogger } from '#lib/logging/logger.js'

  const log = getAppLogger('viewer')

  type Props = {
    /** `cmdr-media://localhost/<token>` URL for the PDF bytes. */
    src: string
    /** File name, surfaced on the embed for assistive tech. */
    fileName: string
    /** The bytes are still on their way from a volume, however long that takes. */
    arriving?: boolean
  }

  const { src, fileName, arriving = false }: Props = $props()

  // `<embed>` doesn't reliably fire `load` (and almost never `error`) in WebKit, so the
  // spinner can't depend on its events alone or it would hang forever over a PDF that
  // already rendered. PDFKit paints fast, so clear the spinner on a short bounded
  // fallback, and sooner if `load` does fire. A genuinely broken PDF (or a 404 from the
  // scheme) surfaces PDFKit's own in-embed message rather than a separate error state.
  // From a volume, PDFKit paints nothing until the backend has read the bytes, so the
  // backend's own report (`arriving`) holds the spinner past the timer.
  let loaded = $state(false)

  $effect(() => {
    const timer = setTimeout(() => {
      loaded = true
    }, 600)
    return () => { clearTimeout(timer); }
  })

  $effect(() => {
    let unlisten: (() => void) | undefined
    let gone = false
    void onViewerEditAction(({ action }) => {
      viewerForwardEditActionToPage(action).catch((error: unknown) => {
        log.warn('Forwarding {action} to the PDF failed: {error}', { action, error })
      })
    }).then((stop) => {
      if (gone) stop()
      else unlisten = stop
    })
    return () => {
      gone = true
      unlisten?.()
    }
  })

  function handleLoad(): void {
    loaded = true
  }
</script>

<div class="media-pdf-stage">
  {#if !loaded || arriving}
    <div class="media-status" role="status">
      <Spinner size="lg" />
      <span class="sr-only">{tString('viewer.pdf.loading')}</span>
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
