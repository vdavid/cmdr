<script lang="ts">
    /**
     * The reclaim-space line + button under the importance slider. Lowering the
     * slider is forward-only: it never deletes rows, so a drive indexed at a broad setting
     * keeps that coverage searchable. This surfaces the leftover ("you have N indexed, your
     * setting covers M, the extra K stay searchable") and offers to delete it to free disk.
     *
     * Value first, tradeoff second: the line frames the extra entries as still useful, and
     * the button offers the space-vs-reindex tradeoff — never two sentences in tension (it
     * composes as one narrative with the kept-rows line).
     *
     * Only shown once counts have settled: the parent passes `blocked` while it's waiting on
     * importance or a scan, and the backend `pending` flag guards the rest, so the reclaim
     * line never proposes a destructive number off a lower bound. Deleting is recoverable
     * (a later pass re-indexes anything still covered), which the confirm dialog says.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatFileSize } from '$lib/settings/reactive-settings.svelte'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import { addToast } from '$lib/ui/toast'
    import { getAppLogger } from '$lib/logging/logger'
    import { getEnabledMediaIndexVolumeIds } from '$lib/media-index/enabled-volumes'
    import { mediaIndexReclaimPreview, mediaIndexPruneBelowThreshold, type ReclaimPreview } from '$lib/tauri-commands'
    import Button from '$lib/ui/Button.svelte'
    import { shouldOfferReclaim } from './media-index-reclaim'

    const log = getAppLogger('media-index')

    interface Props {
        /** The current (settled) importance threshold to compute leftover coverage at. */
        threshold: number
        /** Suppress the line while counts are unsettled (waiting on importance / a scan). */
        blocked: boolean
    }
    const { threshold, blocked }: Props = $props()

    // The reclaim split for the current threshold (`null` until the first result lands).
    let preview = $state<ReclaimPreview | null>(null)
    let pruning = $state(false)
    // Monotonic id so a late preview for a superseded threshold is dropped.
    let previewSeq = 0

    async function refreshPreview(): Promise<void> {
        const seq = ++previewSeq
        try {
            const result = await mediaIndexReclaimPreview(threshold, getEnabledMediaIndexVolumeIds())
            if (seq !== previewSeq) return
            preview = result
        } catch (err) {
            if (seq !== previewSeq) return
            preview = null
            log.warn('reclaim-preview query failed: {err}', { err: String(err) })
        }
    }

    // Refetch when the threshold changes or the block clears; drop the stale count while
    // blocked so a superseded number never flashes.
    $effect(() => {
        void threshold
        if (blocked) {
            preview = null
            return
        }
        void refreshPreview()
    })

    // Offer the line only when counts have settled AND the leftover is meaningfully large.
    const offer = $derived(
        !blocked && preview !== null && !preview.pending && shouldOfferReclaim(preview.totalStored, preview.doomedCount),
    )

    async function handleReclaim(): Promise<void> {
        if (!preview || pruning) return
        const confirmed = await confirmDialog(
            tString('settings.mediaIndex.reclaim.confirmBody', {
                doomed: preview.doomedCount,
                doomedText: formatInteger(preview.doomedCount),
                size: formatFileSize(preview.estimatedBytes),
            }),
            tString('settings.mediaIndex.reclaim.confirmTitle'),
        )
        if (!confirmed) return
        pruning = true
        try {
            const result = await mediaIndexPruneBelowThreshold(threshold, getEnabledMediaIndexVolumeIds())
            if (result.deletedRows > 0) {
                addToast(tString('settings.mediaIndex.reclaim.freed', { size: formatFileSize(result.freedBytes) }), {
                    level: 'success',
                })
            } else {
                addToast(tString('settings.mediaIndex.reclaim.alreadyCleared'), { level: 'info' })
            }
            await refreshPreview()
        } catch (err) {
            log.warn('reclaim prune failed: {err}', { err: String(err) })
            addToast(tString('settings.mediaIndex.reclaim.couldNotDelete'), { level: 'warn' })
        } finally {
            pruning = false
        }
    }
</script>

{#if offer && preview}
    <div class="mi-reclaim">
        <p class="mi-reclaim-line">
            {tString('settings.mediaIndex.reclaim.line', {
                totalText: formatInteger(preview.totalStored),
                coveredText: formatInteger(preview.coveredStored),
                doomed: preview.doomedCount,
                doomedText: formatInteger(preview.doomedCount),
            })}
        </p>
        <Button size="mini" onclick={handleReclaim} disabled={pruning}>
            {tString('settings.mediaIndex.reclaim.button', {
                doomed: preview.doomedCount,
                size: formatFileSize(preview.estimatedBytes),
            })}
        </Button>
    </div>
{/if}

<style>
    .mi-reclaim {
        margin-top: var(--spacing-sm);
        padding-top: var(--spacing-sm);
        border-top: 1px solid var(--color-border-subtle);
    }

    .mi-reclaim-line {
        margin: 0 0 var(--spacing-xs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }
</style>
