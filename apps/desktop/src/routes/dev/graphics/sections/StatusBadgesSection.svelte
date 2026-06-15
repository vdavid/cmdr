<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Badge {
        file: string
        caption: string
        usage: string
    }

    const BADGES: Badge[] = [
        {
            file: 'sync-synced',
            caption: 'sync-synced',
            usage: 'Cloud-sync state badge overlaid on a file icon when the local copy is fully synced.',
        },
        {
            file: 'sync-uploading',
            caption: 'sync-uploading',
            usage: 'Cloud-sync state badge overlaid on a file icon while the local copy is uploading.',
        },
        {
            file: 'sync-downloading',
            caption: 'sync-downloading',
            usage: 'Cloud-sync state badge overlaid on a file icon while the remote copy is downloading.',
        },
        {
            file: 'sync-online-only',
            caption: 'sync-online-only',
            usage: 'Cloud-sync state badge overlaid on a file icon when the file lives online only.',
        },
        {
            file: 'mobile-device',
            caption: 'mobile-device',
            usage: 'Device glyph for a connected MTP phone in the volume breadcrumb.',
        },
    ]
</script>

<SectionCard id="graphics-status-badges" label="Status badges">
    <p class="intro">
        Fixed-color state badges served as <code>&lt;img&gt;</code> from <code>/icons/</code>. They carry their own
        colors (not <code>currentColor</code>), which is why they live outside the <code>Icon</code> registry. Shown at a
        24px review size; in the app the sync badges overlay file icons at about 10px.
    </p>
    <div class="grid">
        {#each BADGES as badge (badge.file)}
            <div class="cell" use:tooltip={badge.usage}>
                <div class="badge-host">
                    <img src={`/icons/${badge.file}.svg`} alt="" width="24" height="24" />
                </div>
                <p class="caption">{badge.caption}</p>
            </div>
        {/each}
    </div>
</SectionCard>

<style>
    .intro {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .intro code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
        gap: var(--spacing-lg);
    }

    .cell {
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    .badge-host {
        height: 48px;
        display: flex;
        align-items: center;
    }

    .caption {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        color: var(--color-text-tertiary);
        text-align: center;
    }
</style>
