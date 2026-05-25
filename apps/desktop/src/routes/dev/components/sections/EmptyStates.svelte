<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
</script>

<SectionCard id="components-empty-states" label="Empty states">
    <p class="empty">Empty folder</p>
</SectionCard>

<style>
    .empty {
        margin: 0;
        padding: var(--spacing-2xl);
        text-align: center;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
