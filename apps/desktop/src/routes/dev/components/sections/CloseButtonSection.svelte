<script lang="ts">
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import CloseButton from '#lib/ui/CloseButton.svelte'

    function ignore(): void {}
</script>

<SectionCard id="components-close-button" label="Close button">
    <div class="rows">
        <div class="row">
            <span class="row-label">Normal</span>
            <CloseButton ariaLabel="Dismiss" onclick={ignore} />
            <span class="row-note">Hover for the tint; toasts and inline notices (the Ask Cmdr nudge) use it</span>
        </div>
        <div class="row">
            <span class="row-label">Pinned in a notice</span>
            <div class="notice">
                <CloseButton class="notice-close" ariaLabel="Dismiss" onclick={ignore} />
                <span>The caller places it with a <code>class</code> and a <code>:global(...)</code> rule.</span>
            </div>
        </div>
    </div>
</SectionCard>

<style>
    .rows {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md);
    }

    .row {
        display: flex;
        gap: var(--spacing-lg);
        align-items: center;
        flex-wrap: wrap;
    }

    .row-label {
        min-width: 200px;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .row-note {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .notice {
        position: relative;
        max-width: 320px;
        padding: var(--spacing-sm) var(--spacing-xl) var(--spacing-sm) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .notice :global(.notice-close) {
        position: absolute;
        top: calc(var(--spacing-sm) - 3px);
        right: calc(var(--spacing-sm) - 3px);
    }
</style>
