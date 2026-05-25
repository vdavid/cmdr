<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    const longPath = '/Users/david/Library/CloudStorage/Dropbox/obsidian/40-49 things/veszelovszki.com and other domains.md'
</script>

<SectionCard id="components-tooltips" label="Tooltips">
    <div class="grid">
        <div class="cell">
            <p class="caption">Plain text</p>
            <span class="anchor" use:tooltip={'Close tab'}>Hover me</span>
        </div>

        <div class="cell">
            <p class="caption">With shortcut</p>
            <span class="anchor" use:tooltip={{ text: 'New tab', shortcut: '⌘T' }}>Hover me</span>
        </div>

        <div class="cell">
            <p class="caption">Rich HTML</p>
            <span
                class="anchor"
                use:tooltip={{ html: 'Line one<br>Line two with <span class="size-mb">1.02 MB</span>' }}
            >
                Hover me
            </span>
        </div>

        <div class="cell">
            <p class="caption">Overflow only</p>
            <span class="anchor truncated" use:tooltip={{ text: longPath, overflowOnly: true }}>{longPath}</span>
        </div>
    </div>
</SectionCard>

<style>
    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
        gap: var(--spacing-lg);
    }

    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .anchor {
        display: inline-block;
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .truncated {
        max-width: 120px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
