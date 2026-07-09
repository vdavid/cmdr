<script lang="ts">
    /**
     * Explicitly-approximate compressed-size estimate for the Compress dialog.
     *
     * The backend samples the estimate once (at deflate level 6) during the
     * deep scan and ships it as per-class subtotals; this line re-scales it to
     * the currently-selected compression level via `scaleCompressedEstimate`
     * with NO re-scan (it subscribes to the same `behavior.archiveCompressionLevel`
     * setting the slider writes, so moving the slider updates the number live).
     *
     * States: a value when the estimate is present, a subtle loading affordance
     * while a LOCAL scan is still running, and nothing otherwise — remote
     * (SMB/MTP) scans never sample, so an absent estimate is honest. Kept out of
     * `TransferDialog.svelte` so that file's line budget stays put.
     */
    import { onMount } from 'svelte'
    import Size from '$lib/ui/Size.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import type { CompressedSizeEstimate } from '$lib/tauri-commands'
    import { scaleCompressedEstimate } from './compress-estimate-scaling'

    interface Props {
        /** Per-class level-6 estimate, or `null` while scanning / when suppressed. */
        estimate: CompressedSizeEstimate | null
        /** Whether the deep scan is still running (drives the loading affordance). */
        isScanning: boolean
        /** Local source? Only local scans sample, so the loading affordance is
         *  suppressed for remote sources (which never produce an estimate). */
        sourceIsLocal: boolean
    }

    const { estimate, isScanning, sourceIsLocal }: Props = $props()

    const LEVEL_ID = 'behavior.archiveCompressionLevel'
    let level = $state(getSetting(LEVEL_ID) as number)
    onMount(() =>
        onSpecificSettingChange(LEVEL_ID, (_id, newValue) => {
            level = newValue as number
        }),
    )

    const scaledBytes = $derived(estimate ? Math.round(scaleCompressedEstimate(estimate, level)) : null)
    const showLoading = $derived(sourceIsLocal && isScanning && estimate === null)
</script>

{#if scaledBytes !== null}
    <p class="estimate">
        <span
            class="estimate-label"
            use:tooltip={{ text: tString('fileOperations.transferDialog.estimatedSizeTooltip') }}
        >
            {tString('fileOperations.transferDialog.estimatedSize')}
        </span>
        <span class="estimate-value">~ <Size bytes={scaledBytes} /></span>
    </p>
{:else if showLoading}
    <p class="estimate">
        <span class="estimate-label">{tString('fileOperations.transferDialog.estimatedSize')}</span>
        <Spinner size="sm" />
    </p>
{/if}

<style>
    .estimate {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .estimate-label {
        color: var(--color-text-tertiary);
    }

    .estimate-value {
        font-variant-numeric: tabular-nums;
        color: var(--color-text-primary);
    }
</style>
