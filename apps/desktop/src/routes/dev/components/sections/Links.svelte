<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { openExternalUrl } from '$lib/tauri-commands'

    const supportEmail = 'support@getcmdr.com'
    const supportHref = `mailto:${supportEmail}`

    function noop() {
        // Catalog stub: real consumers wire up a real handler.
    }

    async function openSupport(event: MouseEvent) {
        event.preventDefault()
        try {
            await openExternalUrl(supportHref)
        } catch (error) {
            // eslint-disable-next-line no-console -- dev-only catalog; surface failure to console when outside Tauri
            console.warn('Catalog: openExternalUrl failed (likely outside Tauri):', error)
        }
    }
</script>

<SectionCard id="components-links" label="Links">
    <div class="rows">
        <div class="row">
            <span class="row-label">In-app button</span>
            <LinkButton onclick={noop}>Open settings</LinkButton>
        </div>

        <div class="row">
            <span class="row-label">External href</span>
            <LinkButton href={supportHref} onclick={openSupport}>{supportEmail}</LinkButton>
        </div>

        <div class="row">
            <span class="row-label">Inline in prose</span>
            <p class="prose">
                Read more in the <LinkButton onclick={noop}>docs</LinkButton> to learn how to do X.
            </p>
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
        align-items: baseline;
    }

    .row-label {
        min-width: 140px;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .prose {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }
</style>
