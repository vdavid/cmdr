<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        /** Optional label rendered as an `<h3>` above the card. Omitted for unlabelled groupings. */
        label?: string
        /** Optional id on the outer `<section>` for scroll anchoring. */
        id?: string
        /**
         * Dim the card to signal a closed gate (e.g. FDA-pending). Emits
         * `data-gated="true"` on the wrapper and fades the inner `.section-card`.
         * Inner controls own their own `disabled` state; this only owns the
         * visual cue. Default `false` (attribute omitted entirely).
         */
        gated?: boolean
        children: Snippet
    }

    const { label, id, gated = false, children }: Props = $props()
</script>

<section class="section-card-wrap" {id} data-gated={gated ? 'true' : undefined}>
    {#if label}
        <h3 class="section-card-label">{label}</h3>
    {/if}
    <div class="section-card">
        {@render children()}
    </div>
</section>

<style>
    .section-card-wrap {
        margin-bottom: var(--spacing-xl);
    }

    .section-card-wrap:last-child {
        margin-bottom: 0;
    }

    .section-card-label {
        margin: 0 0 var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .section-card {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-lg);
        border: 1px solid var(--color-border-subtle);
    }

    /* Gated cards dim their content. Inner controls own their own `disabled`
       state; the wrapper only owns the visual cue. */
    .section-card-wrap[data-gated='true'] .section-card {
        opacity: 0.5;
    }
</style>
