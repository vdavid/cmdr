<script lang="ts">
    /**
     * What a planned Copy, Move, or Delete costs on an S3 provider's bill, at its
     * LIST prices: "About $0.02 at AWS list prices", one line per provider, then
     * one ⓘ saying free tiers, discounts, and minimums can move it.
     *
     * Asks the backend once per non-null `request` (built by `costRequestFor`),
     * drops an answer to a request that's no longer current, hides an estimate
     * that rounds to zero, and renders nothing on a refusal. Never on a rollback
     * or an undo: those just run. `DETAILS.md` § "S3 cost line".
     */
    import { estimateOperationCost, type CostEstimateRequest } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'
    import InfoTip from '$lib/ui/InfoTip.svelte'
    import { visibleCostLines, type CostLine } from './s3-cost-line'

    interface Props {
        /** What to price, or `null` for "don't ask yet" (scan running, no preview id, unpriced operation). */
        request: CostEstimateRequest | null
    }

    const { request }: Props = $props()

    const log = getAppLogger('s3CostLine')

    let lines = $state<CostLine[]>([])

    /** By value, so a host re-deriving an equal request object doesn't ask again. */
    const requestKey = $derived(request === null ? null : JSON.stringify(request))

    $effect(() => {
        // Rebuilt from the key, so the effect tracks nothing but the key.
        const current = requestKey === null ? null : (JSON.parse(requestKey) as CostEstimateRequest)
        lines = []
        if (current === null) return
        let stale = false
        estimateOperationCost(current)
            .then((estimates) => {
                if (!stale) lines = visibleCostLines(estimates)
            })
            .catch((error: unknown) => {
                log.debug('No S3 cost estimate for preview {previewId}: {error}', {
                    previewId: current.previewId,
                    error,
                })
            })
        return () => {
            stale = true
        }
    })
</script>

{#if lines.length > 0}
    <div class="s3-cost">
        <div class="s3-cost-lines">
            {#each lines as line (line.providerLabel)}
                <p class="s3-cost-amount">
                    {tString('fileOperations.s3Cost.line', { amount: line.amountText, provider: line.providerLabel })}
                </p>
            {/each}
        </div>
        <InfoTip label={tString('fileOperations.s3Cost.infoLabel')} text={tString('fileOperations.s3Cost.infoText')} />
    </div>
{/if}

<style>
    /* Right-aligned, so it reads as part of the scan tallies it sits under. */
    .s3-cost {
        display: flex;
        align-items: flex-end;
        justify-content: flex-end;
        gap: var(--spacing-xs);
        text-align: end;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .s3-cost-lines {
        display: flex;
        flex-direction: column;
    }

    .s3-cost-amount {
        margin: 0;
        font-variant-numeric: tabular-nums;
    }
</style>
