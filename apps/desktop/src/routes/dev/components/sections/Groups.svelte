<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
</script>

<SectionCard id="components-groups" label="Groups">
    <div class="examples">
        <div>
            <p class="example-caption">With label</p>
            <div class="inner-wrap">
                <SectionCard label="Theme">
                    <p class="placeholder">Card body content.</p>
                </SectionCard>
            </div>
        </div>

        <div>
            <p class="example-caption">Without label</p>
            <div class="inner-wrap">
                <SectionCard>
                    <p class="placeholder">Card body content.</p>
                </SectionCard>
            </div>
        </div>

        <div>
            <p class="example-caption">Adjacent cards</p>
            <div class="inner-wrap">
                <SectionCard label="First card">
                    <p class="placeholder">First card body.</p>
                </SectionCard>
                <SectionCard label="Second card">
                    <p class="placeholder">Second card body.</p>
                </SectionCard>
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .examples {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
    }

    .example-caption {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    /* Nested cards: give them a tertiary background so they stand out against the
       outer SectionCard's secondary surface. */
    .inner-wrap :global(.section-card) {
        background: var(--color-bg-tertiary);
    }

    .placeholder {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
