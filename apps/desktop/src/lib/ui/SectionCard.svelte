<script lang="ts" module>
    /**
     * A card's tone. `neutral` is the plain grouping surface; the other three tint the
     * fill and border to mark a block as informational, cautionary, or destructive.
     * The tone colors the SURFACE only: text inside keeps its normal color, so a tinted
     * card doesn't turn into a wall of colored type.
     */
    export type SectionCardTone = 'neutral' | 'info' | 'warning' | 'error'
</script>

<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        /** Optional label rendered as an `<h3>` above the card. Omitted for unlabelled groupings. */
        label?: string
        /** Optional inline element (e.g. a status badge) rendered next to the label. Needs a label. */
        badge?: Snippet
        /** Optional id on the outer `<section>` for scroll anchoring. */
        id?: string
        /**
         * Dim the card to signal a closed gate (e.g. FDA-pending). Emits
         * `data-gated="true"` on the wrapper and fades the inner `.section-card`.
         * Inner controls own their own `disabled` state; this only owns the
         * visual cue. Default `false` (attribute omitted entirely).
         */
        gated?: boolean
        /** Tints the card's fill and border. Default `neutral` (the plain grouping surface). */
        tone?: SectionCardTone
        children: Snippet
    }

    const { label, badge, id, gated = false, tone = 'neutral', children }: Props = $props()
</script>

<section class="section-card-wrap" {id} data-gated={gated ? 'true' : undefined}>
    {#if label}
        <div class="section-card-header">
            <h3 class="section-card-label">{label}</h3>
            {#if badge}{@render badge()}{/if}
        </div>
    {/if}
    <div class="section-card" data-tone={tone}>
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

    .section-card-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        /* Left inset + bottom gap that the bare label used to carry, so the card
           title still lines up whether or not a badge sits beside it. */
        margin: 0 0 var(--spacing-sm) var(--spacing-md);
    }

    .section-card-label {
        margin: 0;
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

    /* Toned cards. Each uses the OPAQUE tint token, not the translucent one: a card
       often sits on a modal panel, and a translucent fill lets the window behind it
       bleed through. Border and fill only; text inside keeps its own color. */
    .section-card[data-tone='info'] {
        background: var(--color-info-bg);
        border-color: var(--color-info-border);
    }

    .section-card[data-tone='warning'] {
        background: var(--color-warning-bg-solid);
        border-color: var(--color-warning);
    }

    .section-card[data-tone='error'] {
        background: var(--color-error-bg);
        border-color: var(--color-error-border);
    }

    /* Gated cards dim their content. Inner controls own their own `disabled`
       state; the wrapper only owns the visual cue. */
    .section-card-wrap[data-gated='true'] .section-card {
        opacity: 0.5;
    }
</style>
