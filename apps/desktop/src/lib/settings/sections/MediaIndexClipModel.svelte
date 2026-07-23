<!--
  The Semantic search card body: the on/off toggle plus the on-device CLIP model management,
  inside the "Semantic search" card in `ImageIndexingSection.svelte`.

  Semantic search ("beach sunset" → the photo) needs an on-device CLIP model, downloaded on
  demand and gated to Apple Silicon (the Neural Engine path). One backend atomic
  (`mediaIndex.semanticSearch.enabled`) gates both the search read and the CLIP embedding
  writes, so turning it off stops new work without deleting anything.

  Layout by state:
    - Not supported (non-Apple-Silicon): the toggle is disabled with a short explanation.
    - Supported + toggle on + no model: the Download button (honest "~X MB").
    - Supported + toggle on + model installed: a ready line + a "Delete model (reclaim ~N)"
      button (confirm → delete → reclaim the model's disk + embeddings).
    - Supported + toggle off + model installed: a muted "downloaded but off" note plus the
      same Delete button (reclaiming disk is valid whether or not search is on).

  The heavy work is all backend; this is a thin toggle + status + trigger.
-->
<script lang="ts">
    import { onMount } from 'svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Button from '$lib/ui/Button.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { getSetting, getSettingDefinition, onSpecificSettingChange } from '$lib/settings'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import {
        mediaIndexClipModelStatus,
        mediaIndexDownloadClipModel,
        mediaIndexDeleteClipModel,
        type ClipModelStatus,
    } from '$lib/tauri-commands'

    interface Props {
        searchQuery?: string
    }
    const { searchQuery = '' }: Props = $props()

    const semanticDef = getSettingDefinition('mediaIndex.semanticSearch.enabled') ?? { label: '', description: '' }

    let status = $state<ClipModelStatus | null>(null)
    let downloading = $state(false)
    let downloadFailed = $state(false)
    let deleting = $state(false)
    let deleteFailed = $state(false)

    // Live toggle state, so the model controls reveal/hide the moment it flips (no restart).
    let enabled = $state(getSetting('mediaIndex.semanticSearch.enabled'))

    async function refresh(): Promise<void> {
        try {
            status = await mediaIndexClipModelStatus()
        } catch {
            status = null
        }
    }

    onMount(() => {
        void refresh()
        return onSpecificSettingChange('mediaIndex.semanticSearch.enabled', (_id, value) => (enabled = value))
    })

    async function download(): Promise<void> {
        downloading = true
        downloadFailed = false
        try {
            await mediaIndexDownloadClipModel()
            await refresh()
        } catch {
            downloadFailed = true
        } finally {
            downloading = false
        }
    }

    async function remove(): Promise<void> {
        if (deleting) return
        const confirmed = await confirmDialog(
            tString('settings.mediaIndex.clip.deleteConfirmBody', { size: reclaimText }),
            tString('settings.mediaIndex.clip.deleteConfirmTitle'),
        )
        if (!confirmed) return
        deleting = true
        deleteFailed = false
        try {
            await mediaIndexDeleteClipModel()
            await refresh()
        } catch {
            deleteFailed = true
        } finally {
            deleting = false
        }
    }

    // Round to whole megabytes for the honest "~X MB" download copy (the unit lives in the
    // message, so the number stays locale-formatted).
    const sizeText = $derived(status ? formatInteger(Math.round(status.downloadBytes / 1_000_000)) : '')
    // The reclaimable figure for the delete button + confirm, via the house size formatter.
    const reclaimText = $derived(status ? formatFileSize(status.downloadBytes) : '')

    const unsupported = $derived(status !== null && !status.supported)
    const installed = $derived(status?.installed ?? false)
</script>

<SettingRow
    id="mediaIndex.semanticSearch.enabled"
    label={semanticDef.label}
    description={semanticDef.description}
    {searchQuery}
>
    <SettingSwitch id="mediaIndex.semanticSearch.enabled" disabled={unsupported} />
</SettingRow>

{#if unsupported}
    <p class="cm-note">{tString('settings.mediaIndex.clip.notSupported')}</p>
{:else if status?.supported}
    <div class="clip-model">
        {#if installed}
            {#if enabled}
                <p class="cm-ready">
                    <Icon name="check" size={14} aria-hidden="true" />
                    <span>{tString('settings.mediaIndex.clip.ready')}</span>
                </p>
            {:else}
                <p class="cm-note">{tString('settings.mediaIndex.clip.offButInstalled')}</p>
            {/if}
            <Button size="mini" variant="danger" disabled={deleting} onclick={() => void remove()}>
                {#if deleting}
                    <span class="cm-btn-inner"><Spinner size="sm" /> {tString('settings.mediaIndex.clip.deleting')}</span>
                {:else}
                    {tString('settings.mediaIndex.clip.deleteButton', { size: reclaimText })}
                {/if}
            </Button>
            {#if deleteFailed}
                <p class="cm-note">{tString('settings.mediaIndex.clip.deleteFailed')}</p>
            {/if}
        {:else if enabled}
            {#if !status.configured}
                <p class="cm-note">{tString('settings.mediaIndex.clip.comingSoon')}</p>
            {:else}
                <Button size="mini" disabled={downloading} onclick={() => void download()}>
                    {#if downloading}
                        <span class="cm-btn-inner"
                            ><Spinner size="sm" /> {tString('settings.mediaIndex.clip.downloading')}</span
                        >
                    {:else}
                        <span class="cm-btn-inner"
                            ><Icon name="download" size={14} aria-hidden="true" />
                            {tString('settings.mediaIndex.clip.download', { sizeText })}</span
                        >
                    {/if}
                </Button>
                {#if downloadFailed}
                    <p class="cm-note">{tString('settings.mediaIndex.clip.failed')}</p>
                {/if}
            {/if}
        {/if}
    </div>
{/if}

<style>
    .clip-model {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-sm);
    }

    .cm-note {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .cm-ready {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .cm-btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
