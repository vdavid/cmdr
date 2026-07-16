<!--
  The CLIP semantic-search model control, inside the "Image search" settings card (plan M3).
  Semantic search ("beach sunset" → the photo) needs an on-device CLIP model downloaded on
  demand. This surfaces its state and the download action, gated on Apple Silicon (the
  Neural Engine path) and only shown once image indexing is on.

  States: unsupported hardware → nothing; not published yet → "coming soon"; installed →
  a ready line; otherwise a "Download (~X MB)" button (honest size), with in-progress and
  failed states. The heavy work is all backend; this is a thin status + trigger.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { mediaIndexClipModelStatus, mediaIndexDownloadClipModel, type ClipModelStatus } from '$lib/tauri-commands'

    let status = $state<ClipModelStatus | null>(null)
    let downloading = $state(false)
    let failed = $state(false)

    async function refresh(): Promise<void> {
        try {
            status = await mediaIndexClipModelStatus()
        } catch {
            status = null
        }
    }

    onMount(refresh)

    async function download(): Promise<void> {
        downloading = true
        failed = false
        try {
            await mediaIndexDownloadClipModel()
            await refresh()
        } catch {
            failed = true
        } finally {
            downloading = false
        }
    }

    // Round to whole megabytes for the honest "~X MB" download copy (the unit lives in the
    // message, so the number stays locale-formatted).
    const sizeText = $derived(status ? formatInteger(Math.round(status.downloadBytes / 1_000_000)) : '')
</script>

{#if status?.supported}
    <div class="clip-model">
        <span class="cm-title">{tString('settings.mediaIndex.clip.title')}</span>
        <p class="cm-desc">{tString('settings.mediaIndex.clip.description')}</p>

        {#if status.installed}
            <p class="cm-ready">
                <Icon name="check" size={14} aria-hidden="true" />
                <span>{tString('settings.mediaIndex.clip.ready')}</span>
            </p>
        {:else if !status.configured}
            <p class="cm-note">{tString('settings.mediaIndex.clip.comingSoon')}</p>
        {:else}
            <button type="button" class="cm-download" disabled={downloading} onclick={download}>
                {#if downloading}
                    <Spinner size="sm" />
                    <span>{tString('settings.mediaIndex.clip.downloading')}</span>
                {:else}
                    <Icon name="download" size={14} aria-hidden="true" />
                    <span>{tString('settings.mediaIndex.clip.download', { sizeText })}</span>
                {/if}
            </button>
            {#if failed}
                <p class="cm-failed">{tString('settings.mediaIndex.clip.failed')}</p>
            {/if}
        {/if}
    </div>
{/if}

<style>
    .clip-model {
        margin-top: var(--spacing-md);
        padding-top: var(--spacing-md);
        border-top: 1px solid var(--color-border-subtle);
    }

    .cm-title {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .cm-desc,
    .cm-note {
        margin: var(--spacing-xs) 0 var(--spacing-sm);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .cm-ready,
    .cm-failed {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin: var(--spacing-xs) 0;
        font-size: var(--font-size-sm);
    }

    .cm-ready {
        color: var(--color-text-secondary);
    }

    .cm-failed {
        color: var(--color-text-secondary);
    }

    .cm-download {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-md);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
    }

    .cm-download:hover:not(:disabled) {
        border-color: var(--color-accent);
        background: var(--color-accent-subtle);
    }

    .cm-download:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .cm-download:disabled {
        opacity: 0.7;
        cursor: default;
    }
</style>
