<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import GlyphGrid from '../GlyphGrid.svelte'

    interface Badge {
        /** File stem under `/icons/`. */
        id: string
        caption: string
        usage: string
    }

    const BADGES: Badge[] = [
        {
            id: 'sync-synced',
            caption: 'sync-synced',
            usage: 'Cloud-sync state badge overlaid on a file icon when the local copy is fully synced.',
        },
        {
            id: 'sync-uploading',
            caption: 'sync-uploading',
            usage: 'Cloud-sync state badge overlaid on a file icon while the local copy is uploading.',
        },
        {
            id: 'sync-downloading',
            caption: 'sync-downloading',
            usage: 'Cloud-sync state badge overlaid on a file icon while the remote copy is downloading.',
        },
        {
            id: 'sync-online-only',
            caption: 'sync-online-only',
            usage: 'Cloud-sync state badge overlaid on a file icon when the file lives online only.',
        },
        {
            id: 'mobile-device',
            caption: 'mobile-device',
            usage: 'Device glyph for a connected MTP phone in the volume breadcrumb.',
        },
    ]
</script>

<SectionCard id="graphics-status-badges" label="Status badges">
    <GlyphGrid items={BADGES}>
        {#snippet intro()}
            Fixed-color state badges served as <code>&lt;img&gt;</code> from <code>/icons/</code>. They carry their own
            colors (not <code>currentColor</code>), which is why they live outside the <code>Icon</code> registry. Shown
            at a 24px review size; in the app the sync badges overlay file icons at about 10px.
        {/snippet}
        {#snippet glyph(badge)}
            <img src={`/icons/${badge.id}.svg`} alt="" width="24" height="24" />
        {/snippet}
    </GlyphGrid>
</SectionCard>

<style>
    code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }
</style>
